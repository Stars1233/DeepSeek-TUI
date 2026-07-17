//! Terminal-native underwater field for the CodeWhale transcript.
//!
//! The field is atmosphere, never content: ordinary shell cells share its
//! water column while semantic surfaces such as selections, errors, and code
//! keep their own backgrounds. Reduced motion freezes the field but does not
//! remove it, so choosing an underwater treatment always has a visible result.

use ratatui::{buffer::Buffer, layout::Rect, style::Color};

use crate::palette::{PaletteMode, UiTheme};
use crate::tui::underwater::ShellPhase;

/// Appearance treatment for the underwater shell.
///
/// Parsed once from persisted settings so rendering and scheduling code can
/// branch on typed state instead of scattered string comparisons. Treatment
/// is appearance only: ambient life belongs to every underwater treatment,
/// while motion is governed separately by `low_motion`/`fancy_animations`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OceanTreatment {
    /// State-reactive water column painted from the theme's [`OceanRamp`].
    #[default]
    Ombre,
    /// Plain theme surface with the same state grammar and ambient life.
    Flat,
    /// Legacy full-chrome compatibility shell. Persisted settings normalize
    /// unknown values to ombre, so this is reachable only through explicit
    /// internal selection (tests and future compatibility wiring).
    Classic,
}

impl OceanTreatment {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        let value = value.trim();
        if value.eq_ignore_ascii_case("flat") {
            Self::Flat
        } else if value.eq_ignore_ascii_case("classic") {
            Self::Classic
        } else {
            Self::Ombre
        }
    }

    #[must_use]
    pub fn is_ombre(self) -> bool {
        self == Self::Ombre
    }

    #[must_use]
    pub fn is_flat(self) -> bool {
        self == Self::Flat
    }

    #[must_use]
    pub fn is_classic(self) -> bool {
        self == Self::Classic
    }

    /// Every underwater treatment keeps idle ambient life; only the legacy
    /// classic shell stays still. Flat means a plain surface, not a lifeless
    /// ocean, and Terminal-owned backgrounds still carry foreground life.
    #[must_use]
    pub fn supports_ambient_life(self) -> bool {
        !self.is_classic()
    }
}

/// Minimum empty-water size that earns decorative ambient life. Below this,
/// content and controls own every cell. Shared by the renderer and the idle
/// animation scheduler so redraws are never scheduled for invisible life.
pub const AMBIENT_MIN_WIDTH: u16 = 68;
pub const AMBIENT_MIN_HEIGHT: u16 = 15;

/// Ambient-life inks for a theme, independent of the ombre ramp. Fish use two
/// sunk sky-blue shades so seafoam remains reserved for live work.
#[must_use]
pub fn ambient_inks(theme: &UiTheme) -> (Color, Color) {
    let sky = rgb(theme.info).unwrap_or((106, 174, 242));
    match rgb(theme.surface_bg) {
        Some(base) => (color(mix(sky, base, 0.42)), color(mix(sky, base, 0.28))),
        None => (theme.info, theme.info),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OceanRamp {
    pub surface: Color,
    pub middle: Color,
    pub deep: Color,
    pub ambient: Color,
}

/// One continuous water column shared by every shell band in a frame.
///
/// Individual widgets still own their foreground and semantic surfaces, but
/// ordinary shell backgrounds sample this column with their absolute row.
/// That keeps the header, work strip, transcript, phase line, and composer
/// from each restarting the same miniature gradient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OceanColumn {
    ramp: OceanRamp,
    top: u16,
    height: u16,
    elapsed_ms: u128,
    completion_elapsed_ms: Option<u128>,
    phase: ShellPhase,
    animated: bool,
}

impl OceanColumn {
    #[must_use]
    pub fn new(
        ramp: OceanRamp,
        viewport: Rect,
        elapsed_ms: u128,
        completion_elapsed_ms: Option<u128>,
        phase: ShellPhase,
        animated: bool,
    ) -> Self {
        Self {
            ramp,
            top: viewport.y,
            height: viewport.height.max(1),
            elapsed_ms,
            completion_elapsed_ms,
            phase,
            animated,
        }
    }

    #[must_use]
    pub fn color_at_y(self, y: u16) -> Color {
        let row = y.saturating_sub(self.top).min(self.height - 1);
        if let Some(elapsed) = self.completion_elapsed_ms {
            self.ramp.color_at_completion(row, self.height, elapsed)
        } else if self.animated {
            self.ramp
                .color_at_phase(row, self.height, self.elapsed_ms, self.phase)
        } else {
            self.ramp.color_at(row, self.height)
        }
    }

    #[must_use]
    pub fn with_viewport(mut self, viewport: Rect) -> Self {
        self.top = viewport.y;
        self.height = viewport.height.max(1);
        self
    }

    /// Continue the shared column through a shell-owned surface without
    /// flattening semantic highlights (selection, hover, error, code blocks).
    pub fn paint_matching(self, area: Rect, buf: &mut Buffer, background: Color) {
        for y in area.top()..area.bottom() {
            let row_bg = self.color_at_y(y);
            for x in area.left()..area.right() {
                let cell = &mut buf[(x, y)];
                if cell.bg == background {
                    cell.set_bg(row_bg);
                }
            }
        }
    }
}

impl OceanRamp {
    #[must_use]
    pub fn for_theme(theme: &UiTheme) -> Option<Self> {
        // Solarized Light's Base3 (#fdf6e3) background is part of the named
        // palette's contract. Tinting it with the underwater field turns the
        // shell green-grey and no longer renders Solarized Light (#4457).
        // Keep foreground ambient life, just as the flat treatment does, but
        // leave the canonical background untouched.
        if theme.mode == PaletteMode::SolarizedLight {
            return None;
        }

        let base = rgb(theme.surface_bg)?;
        let seafoam = rgb(theme.accent_secondary).unwrap_or((79, 209, 197));

        let (surface, middle, deep) = match theme.mode {
            PaletteMode::Light => (
                mix(base, seafoam, 0.07),
                mix(base, seafoam, 0.13),
                mix(base, (70, 139, 196), 0.18),
            ),
            PaletteMode::Dark | PaletteMode::Grayscale => (
                mix(base, (30, 71, 103), 0.24),
                mix(base, (7, 30, 54), 0.40),
                mix(base, (2, 9, 24), 0.64),
            ),
            PaletteMode::SolarizedLight => unreachable!("handled above"),
        };

        Some(Self {
            surface: color(surface),
            middle: color(middle),
            deep: color(deep),
            ambient: color(mix(seafoam, base, 0.42)),
        })
    }

    #[must_use]
    pub fn color_at(self, row: u16, height: u16) -> Color {
        if height <= 1 {
            return self.surface;
        }
        let position = f32::from(row.min(height - 1)) / f32::from(height - 1);
        if position <= 0.42 {
            mix_colors(self.surface, self.middle, position / 0.42)
        } else {
            mix_colors(self.middle, self.deep, (position - 0.42) / 0.58)
        }
    }

    #[must_use]
    pub fn color_at_phase(
        self,
        row: u16,
        height: u16,
        elapsed_ms: u128,
        phase: ShellPhase,
    ) -> Color {
        let base = self.color_at(row, height);
        let depth = if height <= 1 {
            0.0
        } else {
            f32::from(row.min(height - 1)) / f32::from(height - 1)
        };
        if matches!(
            phase,
            ShellPhase::Waiting | ShellPhase::Approval | ShellPhase::Failed
        ) {
            return base;
        }
        let cycle = (elapsed_ms % 90_000) as f32 / 90_000.0;
        let breath = (cycle * std::f32::consts::TAU).sin() * 0.5 + 0.5;
        let (phase_bias, phase_depth) = match phase {
            ShellPhase::Idle => (0.035, 1.0 - depth),
            ShellPhase::Typing => (0.025, 1.0 - depth),
            ShellPhase::Working => (0.045, 0.35 + depth * 0.65),
            ShellPhase::Verifying => (0.055, 0.65 + (1.0 - depth) * 0.35),
            ShellPhase::Done => (0.018, 1.0 - depth),
            ShellPhase::Waiting | ShellPhase::Approval | ShellPhase::Failed => unreachable!(),
        };
        mix_colors(base, self.ambient, breath * phase_bias * phase_depth)
    }

    #[must_use]
    pub fn color_at_completion(self, row: u16, height: u16, elapsed_ms: u128) -> Color {
        let base = self.color_at(row, height);
        let elapsed = elapsed_ms.min(800) as f32 / 800.0;
        let brightness = if elapsed <= 0.4 {
            0.88 + (1.12 - 0.88) * (elapsed / 0.4)
        } else {
            1.12 + (1.0 - 1.12) * ((elapsed - 0.4) / 0.6)
        };
        scale_color(base, brightness)
    }
}

#[must_use]
fn rgb(value: Color) -> Option<(u8, u8, u8)> {
    match value {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        _ => None,
    }
}

#[must_use]
fn color((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}

#[must_use]
fn mix_colors(from: Color, to: Color, amount: f32) -> Color {
    match (rgb(from), rgb(to)) {
        (Some(from), Some(to)) => color(mix(from, to, amount)),
        _ => from,
    }
}

#[must_use]
fn scale_color(value: Color, brightness: f32) -> Color {
    let Some((r, g, b)) = rgb(value) else {
        return value;
    };
    color((
        (f32::from(r) * brightness).round().clamp(0.0, 255.0) as u8,
        (f32::from(g) * brightness).round().clamp(0.0, 255.0) as u8,
        (f32::from(b) * brightness).round().clamp(0.0, 255.0) as u8,
    ))
}

#[must_use]
fn mix(from: (u8, u8, u8), to: (u8, u8, u8), amount: f32) -> (u8, u8, u8) {
    let amount = amount.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distance(a: Color, b: Color) -> u16 {
        let (ar, ag, ab) = rgb(a).expect("RGB color");
        let (br, bg, bb) = rgb(b).expect("RGB color");
        ar.abs_diff(br) as u16 + ag.abs_diff(bg) as u16 + ab.abs_diff(bb) as u16
    }

    #[test]
    fn whale_ramp_is_perceptibly_deep_not_merely_non_equal() {
        let ramp = OceanRamp::for_theme(&crate::palette::UI_THEME).expect("RGB theme");
        assert!(
            distance(ramp.surface, ramp.deep) >= 32,
            "the selected underwater treatment must read at a glance"
        );
        assert_ne!(ramp.color_at(0, 20), ramp.color_at(19, 20));
    }

    #[test]
    fn light_theme_stays_light_enough_for_light_theme_text() {
        let ramp = OceanRamp::for_theme(&crate::palette::LIGHT_UI_THEME).expect("RGB theme");
        let (r, g, b) = rgb(ramp.deep).expect("RGB color");
        assert!(u16::from(r) + u16::from(g) + u16::from(b) > 420);
    }

    #[test]
    fn inherited_terminal_background_reports_no_ramp() {
        let mut theme = crate::palette::UI_THEME;
        theme.surface_bg = Color::Reset;
        assert_eq!(OceanRamp::for_theme(&theme), None);
    }

    #[test]
    fn solarized_light_preserves_its_canonical_base3_background() {
        let theme = crate::palette::SOLARIZED_LIGHT_UI_THEME;

        assert_eq!(theme.surface_bg, Color::Rgb(0xfd, 0xf6, 0xe3));
        assert_eq!(OceanRamp::for_theme(&theme), None);
    }

    #[test]
    fn every_shipped_theme_has_an_intentional_ocean_treatment() {
        use crate::palette::{SELECTABLE_THEMES, ThemeId};

        for id in SELECTABLE_THEMES {
            let ramp = OceanRamp::for_theme(&id.ui_theme());
            if matches!(id, ThemeId::Terminal | ThemeId::SolarizedLight) {
                assert_eq!(
                    ramp,
                    None,
                    "{} must keep its canonical background",
                    id.name()
                );
            } else {
                let ramp = ramp.unwrap_or_else(|| panic!("{} has no ocean ramp", id.name()));
                assert_ne!(
                    ramp.surface,
                    ramp.deep,
                    "{} lost underwater depth",
                    id.name()
                );
            }
        }
    }

    #[test]
    fn treatment_parses_saved_values_and_defaults_to_ombre() {
        assert_eq!(OceanTreatment::parse("flat"), OceanTreatment::Flat);
        assert_eq!(OceanTreatment::parse(" FLAT "), OceanTreatment::Flat);
        assert_eq!(OceanTreatment::parse("classic"), OceanTreatment::Classic);
        assert_eq!(OceanTreatment::parse("ombre"), OceanTreatment::Ombre);
        assert_eq!(OceanTreatment::parse("kelp"), OceanTreatment::Ombre);
        assert_eq!(OceanTreatment::parse(""), OceanTreatment::Ombre);
    }

    #[test]
    fn every_underwater_treatment_keeps_ambient_life() {
        assert!(OceanTreatment::Ombre.supports_ambient_life());
        assert!(OceanTreatment::Flat.supports_ambient_life());
        assert!(!OceanTreatment::Classic.supports_ambient_life());
    }

    #[test]
    fn ambient_ink_matches_sunk_sky_shades_and_survives_reset_surfaces() {
        // RGB themes: fish wear two sunk sky shades; seafoam remains live-work ink.
        let theme = crate::palette::UI_THEME;
        let ramp = OceanRamp::for_theme(&theme).expect("RGB theme");
        let (primary, secondary) = ambient_inks(&theme);
        assert_ne!(primary, ramp.ambient);
        assert_ne!(primary, secondary);
        assert_ne!(primary, theme.accent_secondary);

        // Terminal-owned surfaces have no RGB base; the raw secondary accent
        // lets the terminal's own palette color the life.
        let terminal = crate::palette::TERMINAL_UI_THEME;
        assert_eq!(ambient_inks(&terminal), (terminal.info, terminal.info));
    }

    #[test]
    fn shimmer_is_subtle_and_concentrated_near_the_surface() {
        let ramp = OceanRamp::for_theme(&crate::palette::UI_THEME).expect("RGB theme");
        let surface_a = ramp.color_at_phase(0, 20, 0, ShellPhase::Idle);
        let surface_b = ramp.color_at_phase(0, 20, 3_000, ShellPhase::Idle);
        let deep_a = ramp.color_at_phase(19, 20, 0, ShellPhase::Idle);
        let deep_b = ramp.color_at_phase(19, 20, 3_000, ShellPhase::Idle);

        let surface_shift = distance(surface_a, surface_b);
        assert!(
            (1..=8).contains(&surface_shift),
            "surface shift was {surface_shift}"
        );
        assert_eq!(
            deep_a, deep_b,
            "the floor should stay perceptually anchored"
        );
    }

    #[test]
    fn attention_phases_are_still_and_work_phases_have_distinct_depth_bias() {
        let ramp = OceanRamp::for_theme(&crate::palette::UI_THEME).expect("RGB theme");
        for phase in [
            ShellPhase::Waiting,
            ShellPhase::Approval,
            ShellPhase::Failed,
        ] {
            assert_eq!(
                ramp.color_at_phase(4, 20, 0, phase),
                ramp.color_at_phase(4, 20, 45_000, phase)
            );
        }
        assert_ne!(
            ramp.color_at_phase(10, 20, 22_500, ShellPhase::Working),
            ramp.color_at_phase(10, 20, 22_500, ShellPhase::Verifying)
        );
    }

    #[test]
    fn completion_breath_peaks_once_then_settles() {
        let ramp = OceanRamp::for_theme(&crate::palette::UI_THEME).expect("RGB theme");
        let start = ramp.color_at_completion(0, 20, 0);
        let peak = ramp.color_at_completion(0, 20, 320);
        let settled = ramp.color_at_completion(0, 20, 800);
        assert_ne!(start, peak);
        assert_ne!(peak, settled);
        assert_eq!(settled, ramp.color_at(0, 20));
    }

    #[test]
    fn split_shell_surfaces_share_one_absolute_row_column() {
        let theme = crate::palette::UI_THEME;
        let ramp = OceanRamp::for_theme(&theme).expect("RGB theme");
        let viewport = Rect::new(0, 0, 12, 12);
        let header = Rect::new(0, 0, 12, 2);
        let composer = Rect::new(0, 10, 12, 2);
        let mut buf = Buffer::empty(viewport);
        for y in header.top()..header.bottom() {
            for x in header.left()..header.right() {
                buf[(x, y)].set_bg(theme.header_bg);
            }
        }
        for y in composer.top()..composer.bottom() {
            for x in composer.left()..composer.right() {
                buf[(x, y)].set_bg(theme.composer_bg);
            }
        }
        buf[(4, 10)].set_bg(theme.selection_bg);

        let column = OceanColumn::new(ramp, viewport, 0, None, ShellPhase::Idle, false);
        column.paint_matching(header, &mut buf, theme.header_bg);
        column.paint_matching(composer, &mut buf, theme.composer_bg);

        assert_eq!(buf[(0, 0)].bg, ramp.color_at(0, 12));
        assert_eq!(buf[(0, 11)].bg, ramp.color_at(11, 12));
        assert_ne!(buf[(0, 1)].bg, buf[(0, 10)].bg);
        assert_eq!(
            buf[(4, 10)].bg,
            theme.selection_bg,
            "semantic surfaces must survive the shell ombre pass"
        );
    }
}
