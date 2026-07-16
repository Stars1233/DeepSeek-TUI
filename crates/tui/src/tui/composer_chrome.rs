//! Ocean composer chrome policy.
//!
//! Keeps the prompt roomy by default and sheds padding before content when
//! the terminal is short. Content-driven growth still wins once the user
//! types past the baseline.

use crate::tui::app::ComposerDensity;

/// Top/bottom chrome rows for the quiet rule (TOP border only) or the
/// enclosed panel (TOP + BOTTOM).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComposerChrome {
    pub border_rows: u16,
    pub min_content_rows: usize,
    pub max_total_rows: u16,
}

impl ComposerChrome {
    /// Baseline for the given density. Panel shape gets both borders;
    /// quiet shape keeps a single top rule so the prompt still has a
    /// clear ledge without reading as a card.
    #[must_use]
    pub fn for_density(density: ComposerDensity, enclosed_panel: bool) -> Self {
        let border_rows = if enclosed_panel { 2 } else { 1 };
        let min_content_rows = match density {
            ComposerDensity::Compact => 2,
            ComposerDensity::Comfortable => 3,
            ComposerDensity::Spacious => 4,
        };
        let max_total_rows = match density {
            ComposerDensity::Compact => 7,
            ComposerDensity::Comfortable => 9,
            ComposerDensity::Spacious => 12,
        };
        Self {
            border_rows,
            min_content_rows,
            max_total_rows,
        }
    }

    /// Absolute floor so the composer never collapses to a one-line
    /// afterthought when any vertical room remains.
    #[must_use]
    pub fn absolute_min_total(self) -> u16 {
        u16::try_from(
            self.min_content_rows
                .saturating_add(usize::from(self.border_rows)),
        )
        .unwrap_or(3)
        .max(2)
    }
}

/// Decide how many rows the composer should occupy.
///
/// Compact terminals shed padding (drop forced baseline down toward
/// content) before they shed typed content. When height allows, the
/// density minimum always applies — including the empty quiet composer.
#[must_use]
pub fn desired_height(
    content_lines: usize,
    extra_menu_lines: usize,
    available_height: u16,
    density: ComposerDensity,
    enclosed_panel: bool,
) -> u16 {
    let chrome = ComposerChrome::for_density(density, enclosed_panel);
    let available = available_height.max(1);
    let content = content_lines.max(1);
    let wants_panel = enclosed_panel && available >= 3;

    // Shed padding first: if the full baseline does not fit, fall back to
    // content (+ menu) + whatever border still fits, never inventing a
    // cramped one-row total while two rows are available.
    let baseline_content = if available >= chrome.absolute_min_total() {
        content.max(chrome.min_content_rows)
    } else {
        content
    };

    let border = if wants_panel {
        usize::from(chrome.border_rows)
    } else if available >= 2 {
        1
    } else {
        0
    };

    let total = baseline_content
        .saturating_add(extra_menu_lines)
        .saturating_add(border);
    let max_height = usize::from(available.min(chrome.max_total_rows).max(1));
    total.clamp(1, max_height).try_into().unwrap_or(1)
}

/// Top padding inside the content budget. Keep at least one quiet row below a
/// short prompt when the density baseline has room, instead of bottom-pinning
/// the caret directly against the phase footer. Compact heights naturally
/// report zero padding once the budget collapses.
#[must_use]
pub fn top_padding(content_lines: usize, rows_budget: usize) -> usize {
    let content = content_lines.max(1).min(rows_budget.max(1));
    let spare = rows_budget.saturating_sub(content);
    spare.saturating_add(1) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comfortable_empty_composer_keeps_multi_line_baseline() {
        let height = desired_height(1, 0, 8, ComposerDensity::Comfortable, false);
        assert!(
            height >= 4,
            "expected roomy baseline, got {height} (1 border + 3 content)"
        );
    }

    #[test]
    fn compact_height_sheds_padding_before_content() {
        // Only two rows available: keep a border + one content row rather
        // than forcing the comfortable 3-line baseline.
        let height = desired_height(1, 0, 2, ComposerDensity::Comfortable, false);
        assert_eq!(height, 2);
    }

    #[test]
    fn content_growth_still_expands_past_baseline() {
        let height = desired_height(6, 0, 12, ComposerDensity::Comfortable, false);
        assert!(
            height >= 7,
            "typed content must grow the composer: {height}"
        );
    }

    #[test]
    fn spacious_panel_keeps_four_content_rows() {
        let height = desired_height(1, 0, 12, ComposerDensity::Spacious, true);
        assert!(
            height >= 6,
            "spacious panel = 2 borders + 4 content, got {height}"
        );
    }
}
