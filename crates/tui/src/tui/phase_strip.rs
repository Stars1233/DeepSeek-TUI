//! Live phase band for the underwater shell.
//!
//! The HTML reference attaches activity to the transcript and leaves the
//! composer as the final stable object. That means live phases
//! (working / waiting / approval / failed / done) render **above** the
//! composer, while idle and typing keep a quiet phase line beneath it.
//!
//! Classic shell keeps the legacy footer-below-composer order; this module
//! only decides Ocean placement and paints the one-line band.

use std::borrow::Cow;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::localization::{MessageId, tr};
use crate::tui::{
    app::App,
    history::{HistoryCell, ToolCell, ToolStatus},
    underwater::{ShellPhase, ShellTier, phase_marker},
};

/// Where the phase band sits relative to the composer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseStripPlacement {
    /// Live activity: phase sits on the transcript side of the prompt.
    AboveComposer,
    /// Idle / drafting: quiet phase under the prompt.
    BelowComposer,
}

impl PhaseStripPlacement {
    /// Live phases stay above the composer so the prompt is the bottom
    /// stable object. Idle and typing keep the quiet footer under `❯`.
    #[must_use]
    pub fn for_phase(phase: ShellPhase) -> Self {
        match phase {
            ShellPhase::Working
            | ShellPhase::Verifying
            | ShellPhase::Waiting
            | ShellPhase::Approval
            | ShellPhase::Failed
            | ShellPhase::Done => Self::AboveComposer,
            ShellPhase::Idle | ShellPhase::Typing => Self::BelowComposer,
        }
    }

    #[must_use]
    pub fn is_above_composer(self) -> bool {
        matches!(self, Self::AboveComposer)
    }
}

/// Fixed one-row reservation for the phase band.
#[must_use]
pub fn height() -> u16 {
    1
}

fn span_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.width()).sum()
}

fn truncate_to_width(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    let mut result = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width + 1 > width {
            break;
        }
        result.push(ch);
        used += ch_width;
    }
    result.push('…');
    result
}

/// Compact working detail for the phase band: `run ×N · 12s`.
/// Kept quieter than the classic footer's verbose tool-status line so the
/// transcript owns the ledger and the strip only names the live pulse.
fn working_detail(app: &App) -> Option<String> {
    let mut running = 0usize;
    if let Some(active) = app.active_cell.as_ref() {
        for cell in active.entries() {
            running = running.saturating_add(count_running_tools(cell));
        }
    }
    let secs = app
        .turn_started_at
        .map(|started| started.elapsed().as_secs());
    match (running, secs) {
        (0, Some(secs)) if secs > 0 => Some(format!("{secs}s")),
        (n, Some(secs)) if n > 0 => Some(format!("run ×{n} · {secs}s")),
        (n, None) if n > 0 => Some(format!("run ×{n}")),
        _ => None,
    }
}

fn count_running_tools(cell: &HistoryCell) -> usize {
    let HistoryCell::Tool(tool) = cell else {
        return 0;
    };
    match tool {
        ToolCell::Exploring(explore) => explore
            .entries
            .iter()
            .filter(|entry| matches!(entry.status, ToolStatus::Running))
            .count(),
        other => usize::from(other.status() == Some(ToolStatus::Running)),
    }
}

/// Paint the one-line phase band. Owns phase, optional working detail, cost,
/// and detail-key hints — never route/context (header) or Tasks/To-do
/// (work surface).
pub fn render(area: Rect, buf: &mut Buffer, app: &mut App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let status_toast = app.active_status_toast();
    let phase = ShellPhase::from_app(app);
    let tier = ShellTier::for_chrome_width(area.width);
    Block::default()
        .style(Style::default().bg(app.ui_theme.footer_bg))
        .render(area, buf);

    let (marker, phase_label) = phase_marker(app, phase);
    let phase_style = Style::default().fg(phase.color(app)).add_modifier(
        if matches!(phase, ShellPhase::Waiting | ShellPhase::Approval) {
            Modifier::BOLD
        } else {
            Modifier::empty()
        },
    );
    let mut left = vec![
        Span::styled(marker, phase_style),
        Span::raw(" "),
        Span::styled(phase_label.clone(), phase_style),
    ];

    if tier != ShellTier::Compact
        && matches!(phase, ShellPhase::Working | ShellPhase::Verifying)
        && let Some(detail) = working_detail(app)
    {
        left.push(Span::styled(
            " · ",
            Style::default().fg(app.ui_theme.text_dim),
        ));
        left.push(Span::styled(
            detail,
            Style::default().fg(app.ui_theme.status_working),
        ));
    }

    if tier != ShellTier::Compact
        && let Some(toast) = status_toast.filter(|toast| {
            // Completion may land in the same event drain as an approval
            // denial. Keep unresolved attention/error receipts visible after
            // `done`; only routine informational completion copy yields to the
            // stable done marker.
            let survives_completion = matches!(
                toast.level,
                crate::tui::app::StatusToastLevel::Warning
                    | crate::tui::app::StatusToastLevel::Error
            );
            (phase != ShellPhase::Done || survives_completion)
                && !toast.text.trim().is_empty()
                && toast.text.trim() != phase_label.as_ref()
        })
    {
        left.push(Span::styled(
            " · ",
            Style::default().fg(app.ui_theme.text_dim),
        ));
        left.push(Span::styled(
            truncate_to_width(toast.text.trim(), 40),
            Style::default().fg(crate::tui::ui::status_color(toast.level)),
        ));
    }

    let cost = app.displayed_session_cost_for_currency(app.cost_currency);
    let chip = crate::route_billing::usage_chip(
        app.billing_presentation,
        app.api_provider,
        &app.model,
        cost,
        app.cost_currency,
        None,
    );
    if let crate::route_billing::UsageChip::Money(amount) = chip
        && tier != ShellTier::Compact
    {
        left.push(Span::styled(
            " · ",
            Style::default().fg(app.ui_theme.text_dim),
        ));
        left.push(Span::styled(
            amount,
            Style::default().fg(app.ui_theme.text_muted),
        ));
    }

    // Live phases keep the strip quiet: no detail-key chorus competing with
    // the ledger. Idle/typing may advertise keys on the quiet footer.
    // Hints come from shell_key_routing so advertised chords match handlers;
    // bare letters are never advertised — the composer owns printable keys.
    let right_text: Cow<'static, str> = if PhaseStripPlacement::for_phase(phase).is_above_composer()
    {
        Cow::Borrowed("")
    } else {
        use crate::tui::shell_key_routing::{ShellBindingId, binding, footer_action_hints};
        let hint_keys = tr(app.ui_locale, MessageId::FooterHintKeys);
        let hint_output = tr(app.ui_locale, MessageId::FooterHintOutput);
        let hint_context = tr(app.ui_locale, MessageId::FooterHintContext);
        Cow::Owned(match tier {
            ShellTier::Compact => {
                format!("{}:{hint_keys}", binding(ShellBindingId::Help).footer_chord)
            }
            ShellTier::Normal => footer_action_hints(false)
                .replace("{output}", hint_output.as_ref())
                .replace("{keys}", hint_keys.as_ref()),
            ShellTier::Wide => footer_action_hints(true)
                .replace("{output}", hint_output.as_ref())
                .replace("{context}", hint_context.as_ref())
                .replace("{keys}", hint_keys.as_ref()),
        })
    };

    let right_width = right_text.width();
    let available = usize::from(area.width);
    let left_width = span_width(&left);
    if right_width > 0 && left_width + right_width < available {
        left.push(Span::raw(" ".repeat(available - left_width - right_width)));
        left.push(Span::styled(
            right_text.into_owned(),
            Style::default().fg(app.ui_theme.text_hint),
        ));
    }
    Paragraph::new(Line::from(left)).render(area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        tui::active_cell::ActiveCell,
        tui::app::TuiOptions,
        tui::history::{ExecCell, ExecSource, ToolCell, ToolStatus},
    };
    use ratatui::{Terminal, backend::TestBackend};
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    fn test_app() -> App {
        App::new(
            TuiOptions {
                model: "deepseek-v4-flash".to_string(),
                workspace: PathBuf::from("."),
                config_path: None,
                config_profile: None,
                allow_shell: false,
                use_alt_screen: true,
                use_mouse_capture: false,
                use_bracketed_paste: true,
                max_subagents: 1,
                skills_dir: PathBuf::from("."),
                memory_path: PathBuf::from("memory.md"),
                notes_path: PathBuf::from("notes.txt"),
                mcp_config_path: PathBuf::from("mcp.json"),
                use_memory: false,
                start_in_agent_mode: false,
                skip_onboarding: true,
                yolo: false,
                resume_session_id: None,
                initial_input: None,
            },
            &Config::default(),
        )
    }

    #[test]
    fn live_phases_sit_above_composer_idle_stays_below() {
        assert_eq!(
            PhaseStripPlacement::for_phase(ShellPhase::Working),
            PhaseStripPlacement::AboveComposer
        );
        assert_eq!(
            PhaseStripPlacement::for_phase(ShellPhase::Waiting),
            PhaseStripPlacement::AboveComposer
        );
        assert_eq!(
            PhaseStripPlacement::for_phase(ShellPhase::Approval),
            PhaseStripPlacement::AboveComposer
        );
        assert_eq!(
            PhaseStripPlacement::for_phase(ShellPhase::Failed),
            PhaseStripPlacement::AboveComposer
        );
        assert_eq!(
            PhaseStripPlacement::for_phase(ShellPhase::Done),
            PhaseStripPlacement::AboveComposer
        );
        assert_eq!(
            PhaseStripPlacement::for_phase(ShellPhase::Idle),
            PhaseStripPlacement::BelowComposer
        );
        assert_eq!(
            PhaseStripPlacement::for_phase(ShellPhase::Typing),
            PhaseStripPlacement::BelowComposer
        );
    }

    #[test]
    fn working_marker_uses_the_live_seafoam_role() {
        let app = test_app();
        assert_eq!(
            ShellPhase::Working.color(&app),
            app.ui_theme.accent_secondary
        );
        assert_ne!(ShellPhase::Working.color(&app), app.ui_theme.info);
    }

    #[test]
    fn working_band_names_run_count_without_key_chorus() {
        let mut app = test_app();
        app.ui_locale = crate::localization::Locale::En;
        app.is_loading = true;
        app.turn_started_at = Some(Instant::now() - Duration::from_secs(12));
        let mut active = ActiveCell::new();
        active.push_tool(
            "exec-1",
            HistoryCell::Tool(ToolCell::Exec(ExecCell {
                // A build, not a test run — `cargo test` would truthfully
                // classify as the `verifying` phase (ShellPhase::Verifying).
                command: "cargo build -p tui".to_string(),
                status: ToolStatus::Running,
                output: None,
                live_output: None,
                shell_task_id: None,
                owner_agent_id: None,
                owner_agent_name: None,
                started_at: app.turn_started_at,
                duration_ms: None,
                source: ExecSource::Assistant,
                interaction: None,
                output_summary: None,
            })),
        );
        app.active_cell = Some(active);

        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame.area(), frame.buffer_mut(), &mut app))
            .expect("draw");
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("working"), "{text}");
        assert!(text.contains("run ×1"), "{text}");
        assert!(
            !text.contains("Alt+?") && !text.contains("F1:"),
            "live phase strip stays quiet: {text}"
        );
    }

    #[test]
    fn completed_band_keeps_unresolved_warning_visible() {
        let mut app = test_app();
        app.runtime_turn_status = Some("completed".to_string());
        app.push_status_toast(
            "Auto-denied exec_shell: denied earlier; restart Codewhale",
            crate::tui::app::StatusToastLevel::Warning,
            Some(12_000),
        );

        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(frame.area(), frame.buffer_mut(), &mut app))
            .expect("draw");
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(text.contains("done"), "completion phase missing: {text}");
        assert!(
            text.contains("Auto-denied exec_shell"),
            "completion hid unresolved warning: {text}"
        );
    }
}
