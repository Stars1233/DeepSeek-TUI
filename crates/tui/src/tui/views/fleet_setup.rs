//! `/fleet setup` — a progressive "set up your agent team" flow.
//!
//! Replaces the old six-column config matrix (#3791). Fleet is presented as an
//! agent team: the user makes one focused choice at a time (a role, then a model
//! class) and then reviews the full posture — model/route, permissions, tools,
//! workspace/org scope, and review policy — before starting. "Start" inserts a
//! safe profile-authoring prompt into the composer; nothing is written to disk,
//! preserving the existing InsertText-to-compose commit path.
//!
//! NOTE (audit #7 / #3167): the role/model taxonomy and copy below are
//! intentionally English for now; #3167 reworks this into an interactive
//! provider/model picker that will churn most of this text. The command entry
//! (`CmdFleetDescription`) is already localized.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Widget, Wrap},
};

use crate::config::Config;
use crate::palette;
use crate::tui::app::App;
use crate::tui::views::{
    ActionHint, CommandPaletteAction, ModalKind, ModalView, ViewAction, ViewEvent,
    centered_modal_area, render_modal_footer, render_modal_surface, truncate_view_text,
};

const PROFILE_DIR: &str = ".codewhale/agents";

/// A selectable choice in a wizard step: a short identifier `label`, a one-line
/// `summary`, and a longer `description` shown (wrapped) in the detail pane.
struct Choice {
    label: &'static str,
    summary: &'static str,
    description: &'static str,
}

/// Agent-team roles. `label` doubles as the profile `role_hint` and file stem,
/// so these strings are part of the generated-profile contract.
const ROLES: [Choice; 8] = [
    Choice {
        label: "manager",
        summary: "Plan & split queued work",
        description: "Coordinates the Fleet run: plans the work, splits it into bounded tasks, and dispatches workers.",
    },
    Choice {
        label: "main",
        summary: "Default orchestrator",
        description: "The parent for the whole Fleet. Owns topology and verifies the claims workers return.",
    },
    Choice {
        label: "scout",
        summary: "Read-first research",
        description: "Research and repo reconnaissance. Reads and summarizes before anything is written.",
    },
    Choice {
        label: "builder",
        summary: "Implements bounded changes",
        description: "Implements changes strictly inside its assigned task scope; writes only what the slice needs.",
    },
    Choice {
        label: "reviewer",
        summary: "Read-only review",
        description: "Checks regressions, tests, and diffs. Read-only — it never writes.",
    },
    Choice {
        label: "verifier",
        summary: "Runs focused validation",
        description: "Runs targeted validation and reports receipts back to the orchestrator.",
    },
    Choice {
        label: "synthesizer",
        summary: "Reduce receipts to handoff",
        description: "Turns worker receipts into bounded handoff state instead of raw transcript replay.",
    },
    Choice {
        label: "custom",
        summary: "Author a profile by hand",
        description: "Define the posture yourself in a workspace agent TOML profile under .codewhale/agents/.",
    },
];

/// Model-routing classes. `label` is mapped to a profile `model_class_hint` by
/// [`model_class_hint`]; default is `inherit` (reuse the active route).
const MODEL_CLASSES: [Choice; 6] = [
    Choice {
        label: "inherit",
        summary: "Same model as now",
        description: "Reuse the active provider, model, and reasoning for this worker. Recommended default.",
    },
    Choice {
        label: "fast",
        summary: "Low-latency scout",
        description: "An opt-in low-latency class for wide fan-out and quick reconnaissance.",
    },
    Choice {
        label: "balanced",
        summary: "Everyday build/review",
        description: "A balanced class for normal build and review work.",
    },
    Choice {
        label: "strong",
        summary: "Hard problems",
        description: "The strongest class for security, release, and architecture work.",
    },
    Choice {
        label: "deep-reasoning",
        summary: "More reasoning",
        description: "Higher reasoning effort when the active route supports it.",
    },
    Choice {
        label: "tool-heavy",
        summary: "Operator workflows",
        description: "Shell- and artifact-heavy operator workflows.",
    },
];

#[derive(Debug, Clone)]
pub struct FleetSetupSnapshot {
    workspace: PathBuf,
    locale: crate::localization::Locale,
    /// Whether the active provider has a key or local runtime — gates the
    /// model-draft offer, mirroring the constitution card's `provider_ready`.
    provider_ready: bool,
    provider: String,
    model: String,
    reasoning: String,
    subagents_enabled: bool,
    max_subagents: usize,
    launch_concurrency: usize,
    max_admitted: usize,
    subagent_spawn_depth: u32,
    fleet_spawn_depth: u32,
    token_budget: Option<u64>,
    api_timeout_secs: u64,
    heartbeat_timeout_secs: u64,
    /// Lowercased roster member ids with their origin labels (built-in /
    /// config / project), so the wizard can say when a chosen role would
    /// override an existing roster member.
    roster_members: Vec<(String, String)>,
}

impl FleetSetupSnapshot {
    #[must_use]
    pub fn from_app(app: &App, config: &Config) -> Self {
        let provider = app.api_provider.display_name().to_string();
        let model = if app.auto_model {
            app.last_effective_model
                .as_deref()
                .map(|effective| format!("auto -> {effective}"))
                .unwrap_or_else(|| "auto".to_string())
        } else {
            app.model.clone()
        };
        let fleet_spawn_depth = config
            .fleet
            .as_ref()
            .map(|fleet| fleet.exec.max_spawn_depth)
            .unwrap_or_else(|| codewhale_config::FleetExecConfig::default().max_spawn_depth)
            .min(codewhale_config::MAX_SPAWN_DEPTH_CEILING);
        let roster_members =
            crate::fleet::roster::FleetRoster::load(&config.fleet_config(), &app.workspace)
                .members()
                .iter()
                .map(|member| (member.id.to_lowercase(), member.origin.to_string()))
                .collect();

        Self {
            workspace: app.workspace.clone(),
            locale: app.ui_locale,
            provider_ready: crate::config::has_api_key_for(config, app.api_provider),
            provider,
            model,
            reasoning: app.reasoning_effort_display_label(),
            subagents_enabled: config.subagents_enabled_for_provider(app.api_provider),
            max_subagents: config.max_subagents_for_provider(app.api_provider),
            launch_concurrency: config.launch_concurrency_for_provider(app.api_provider),
            max_admitted: config.max_admitted_subagents_for_provider(app.api_provider),
            subagent_spawn_depth: config.subagent_max_spawn_depth_for_provider(app.api_provider),
            fleet_spawn_depth,
            token_budget: config.subagent_token_budget_for_provider(app.api_provider),
            api_timeout_secs: config.subagent_api_timeout_secs_for_provider(app.api_provider),
            heartbeat_timeout_secs: config
                .subagent_heartbeat_timeout_secs_for_provider(app.api_provider),
            roster_members,
        }
    }
}

/// Which focused screen of the wizard is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Pick the team role.
    Role,
    /// Pick the model-routing class.
    Model,
    /// Review the full posture and start.
    Review,
}

pub struct FleetSetupView {
    snapshot: FleetSetupSnapshot,
    step: Step,
    role_idx: usize,
    model_idx: usize,
    review_scroll: usize,
    /// A model-drafted profile awaiting ratification (already sanitized and
    /// bounded by the untrusted gate). Cleared when the selection changes so
    /// a stale draft can never be ratified against fresh answers.
    model_draft: Option<Box<crate::fleet::profile::FleetProfileDraft>>,
    /// Display label of the model that authored `model_draft`.
    model_draft_label: Option<String>,
}

impl FleetSetupView {
    #[must_use]
    pub fn new(app: &App, config: &Config) -> Self {
        Self::from_snapshot(FleetSetupSnapshot::from_app(app, config))
    }

    fn from_snapshot(snapshot: FleetSetupSnapshot) -> Self {
        Self {
            snapshot,
            step: Step::Role,
            role_idx: 0,
            model_idx: 0,
            review_scroll: 0,
            model_draft: None,
            model_draft_label: None,
        }
    }

    /// Install a sanitized, bounded model draft and return the preview the
    /// host must open in the same breath — that preview is the exact TOML the
    /// ratify keypress would persist.
    pub fn install_model_draft(
        &mut self,
        draft: Box<crate::fleet::profile::FleetProfileDraft>,
        model_label: String,
    ) -> (String, String) {
        let (title, header) = match self.snapshot.locale {
            crate::localization::Locale::ZhHans => (
                format!("Fleet 配置 — 由 {model_label} 起草（按 g 批准）"),
                format!(
                    "# .codewhale/agents/{}\n# 由 {model_label} 起草，并由 CodeWhale 校验与限界。\n# 权限保持在 Fleet 底线：无 shell、无 trust、需审批。\n# 在向导中按 g 之前不会保存任何内容。\n\n",
                    draft.file_name()
                ),
            ),
            _ => (
                format!("Fleet profile — draft by {model_label} (g ratifies)"),
                format!(
                    "# .codewhale/agents/{}\n# Drafted by {model_label}, validated and bounded by CodeWhale.\n# Permissions stay at the fleet floor: no shell, no trust, approval required.\n# Nothing is saved until you press g in the wizard.\n\n",
                    draft.file_name()
                ),
            ),
        };
        let content = format!("{header}{}", draft.render_toml());
        self.model_draft = Some(draft);
        self.model_draft_label = Some(model_label);
        (title, content)
    }

    /// The planner role chosen (drives the profile file name and `role_hint`).
    fn selected_role(&self) -> &'static str {
        ROLES[self.role_idx.min(ROLES.len() - 1)].label
    }

    /// Copy note when the chosen role would override an existing roster
    /// member of the same id (e.g. "overrides built-in reviewer"). A saved
    /// profile shadows lower roster layers rather than adding a new member.
    fn roster_override_note(&self) -> Option<String> {
        let role = self.selected_role().to_lowercase();
        self.snapshot
            .roster_members
            .iter()
            .find(|(id, _)| *id == role)
            .map(|(id, origin)| format!("Overrides the {origin} '{id}' roster member."))
    }

    /// The model class chosen, mapped to a profile schema `model_class_hint`.
    fn selected_model_class(&self) -> &'static str {
        model_class_hint(MODEL_CLASSES[self.model_idx.min(MODEL_CLASSES.len() - 1)].label)
    }

    /// Number of selectable rows on the current step (0 on the review step).
    fn step_len(&self) -> usize {
        match self.step {
            Step::Role => ROLES.len(),
            Step::Model => MODEL_CLASSES.len(),
            Step::Review => 0,
        }
    }

    fn move_up(&mut self) {
        match self.step {
            Step::Role => {
                self.role_idx = self.role_idx.saturating_sub(1);
                self.discard_model_draft();
            }
            Step::Model => {
                self.model_idx = self.model_idx.saturating_sub(1);
                self.discard_model_draft();
            }
            Step::Review => self.review_scroll = self.review_scroll.saturating_sub(1),
        }
    }

    /// A draft is only valid for the answers it was requested against.
    fn discard_model_draft(&mut self) {
        self.model_draft = None;
        self.model_draft_label = None;
    }

    fn move_down(&mut self) {
        match self.step {
            Step::Role => {
                self.role_idx = (self.role_idx + 1).min(self.step_len().saturating_sub(1));
                self.discard_model_draft();
            }
            Step::Model => {
                self.model_idx = (self.model_idx + 1).min(self.step_len().saturating_sub(1));
                self.discard_model_draft();
            }
            Step::Review => self.review_scroll = self.review_scroll.saturating_add(1),
        }
    }

    /// Advance to the next step, or — on the review step — commit by inserting
    /// the profile-authoring prompt into the composer.
    fn advance(&mut self) -> ViewAction {
        match self.step {
            Step::Role => {
                self.step = Step::Model;
                ViewAction::None
            }
            Step::Model => {
                self.step = Step::Review;
                self.review_scroll = 0;
                ViewAction::None
            }
            Step::Review => self.insert_profile_prompt_action(),
        }
    }

    /// Step back toward the first screen. Returns `None` at the first step (the
    /// host closes the modal via Esc instead).
    fn back(&mut self) -> ViewAction {
        match self.step {
            Step::Role => ViewAction::None,
            Step::Model => {
                self.step = Step::Role;
                ViewAction::None
            }
            Step::Review => {
                self.step = Step::Model;
                ViewAction::None
            }
        }
    }

    fn insert_profile_prompt_action(&self) -> ViewAction {
        ViewAction::EmitAndClose(ViewEvent::CommandPaletteSelected {
            action: CommandPaletteAction::InsertText {
                text: self.profile_prompt(),
            },
        })
    }

    /// Build the profile authoring prompt for the current role/model selection.
    fn profile_prompt(&self) -> String {
        profile_authoring_prompt(
            &self.snapshot,
            self.selected_role(),
            self.selected_model_class(),
        )
    }

    /// The action hints for the current step's footer (wrapped by the shared
    /// footer renderer so they can never run off the modal edge).
    fn footer_hints(&self) -> Vec<ActionHint> {
        let mut hints = Vec::new();
        match self.step {
            Step::Role => {
                hints.push(ActionHint::new("↑/↓", "choose"));
                hints.push(ActionHint::new("Enter", "next"));
            }
            Step::Model => {
                hints.push(ActionHint::new("↑/↓", "choose"));
                hints.push(ActionHint::new("Enter", "next"));
                hints.push(ActionHint::new("←", "back"));
            }
            Step::Review => {
                hints.push(ActionHint::new("↑/↓", "scroll"));
                hints.push(ActionHint::new("Enter", "start"));
                if self.model_draft.is_some() {
                    hints.push(ActionHint::new("g", "ratify draft"));
                    hints.push(ActionHint::new("m", "redraft"));
                } else if self.snapshot.provider_ready {
                    hints.push(ActionHint::new("m", "model draft"));
                }
                hints.push(ActionHint::new("←", "back"));
            }
        }
        hints.push(ActionHint::new("Esc", "cancel"));
        hints
    }
}

impl ModalView for FleetSetupView {
    fn kind(&self) -> ModalKind {
        ModalKind::FleetSetup
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => ViewAction::Close,
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                ViewAction::None
            }
            KeyCode::Char('m') if self.step == Step::Review && self.snapshot.provider_ready => {
                ViewAction::Emit(ViewEvent::FleetProfileModelDraftRequested {
                    role: self.selected_role().to_string(),
                    model_class: self.selected_model_class().to_string(),
                    locale: self.snapshot.locale,
                })
            }
            KeyCode::Char('g') if self.step == Step::Review => match self.model_draft.clone() {
                Some(draft) => {
                    ViewAction::EmitAndClose(ViewEvent::FleetProfileDraftCommitRequested { draft })
                }
                None => ViewAction::None,
            },
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l')
                if self.step == Step::Review && self.model_draft.is_some() =>
            {
                // A ratify-ready draft is on screen; Enter should ratify it,
                // not silently start the manual profile-prompt flow and drop
                // the draft.
                match self.model_draft.clone() {
                    Some(draft) => {
                        ViewAction::EmitAndClose(ViewEvent::FleetProfileDraftCommitRequested {
                            draft,
                        })
                    }
                    None => ViewAction::None,
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.advance(),
            KeyCode::Left | KeyCode::Char('h') => self.back(),
            KeyCode::Home => {
                self.review_scroll = 0;
                ViewAction::None
            }
            KeyCode::PageUp => {
                self.review_scroll = self.review_scroll.saturating_sub(8);
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.review_scroll = self.review_scroll.saturating_add(8);
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let popup_area = centered_modal_area(area, 96, 30, 60, 16);
        render_modal_surface(area, popup_area, buf);

        let step_no = match self.step {
            Step::Role => 1,
            Step::Model => 2,
            Step::Review => 3,
        };
        let block = Block::default()
            .title(Line::from(Span::styled(
                " Fleet setup — your agent team ",
                Style::default()
                    .fg(palette::WHALE_ACCENT_PRIMARY)
                    .add_modifier(Modifier::BOLD),
            )))
            .title_bottom(
                Line::from(Span::styled(
                    format!(" Step {step_no}/3 "),
                    Style::default().fg(palette::TEXT_MUTED),
                ))
                .alignment(ratatui::layout::Alignment::Right),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(palette::BORDER_COLOR))
            .style(Style::default().bg(palette::DEEPSEEK_INK))
            .padding(Padding::uniform(1));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        let hints = self.footer_hints();
        let content = render_modal_footer(inner, buf, &hints);

        // Header (intro + breadcrumb) above the step body.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(content);
        self.render_header(chunks[0], buf);

        match self.step {
            Step::Role => {
                let mut context = vec![
                    "Fleet runs sub-agents that delegate work. Pick the role this".to_string(),
                    "team member should play. It becomes the profile role_hint.".to_string(),
                ];
                if let Some(note) = self.roster_override_note() {
                    context.push(note);
                }
                render_choice_step(chunks[1], buf, &ROLES, self.role_idx, &context)
            }
            Step::Model => render_choice_step(
                chunks[1],
                buf,
                &MODEL_CLASSES,
                self.model_idx,
                &[
                    format!(
                        "Current route: {} / {}  ·  reasoning {}",
                        self.snapshot.provider, self.snapshot.model, self.snapshot.reasoning
                    ),
                    format!(
                        "Maps to model_class_hint = {}.",
                        self.selected_model_class()
                    ),
                ],
            ),
            Step::Review => self.render_review(chunks[1], buf),
        }
    }
}

impl FleetSetupView {
    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        let (title, subtitle) = match self.step {
            Step::Role => (
                "Choose a team role",
                "Each Fleet member plays one role in the delegation.",
            ),
            Step::Model => (
                "Choose a model class",
                "How this worker should be routed. Inherit keeps your current model.",
            ),
            Step::Review => (
                "Review & start",
                "Confirm the posture below, then start to author the profile.",
            ),
        };
        let lines = vec![
            Line::from(Span::styled(
                title,
                Style::default().fg(palette::DEEPSEEK_SKY).bold(),
            )),
            Line::from(Span::styled(
                subtitle,
                Style::default().fg(palette::TEXT_MUTED),
            )),
        ];
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .render(area, buf);
    }

    fn render_review(&self, area: Rect, buf: &mut Buffer) {
        let role = &ROLES[self.role_idx.min(ROLES.len() - 1)];
        let model = &MODEL_CLASSES[self.model_idx.min(MODEL_CLASSES.len() - 1)];
        let (profile_value, _) = profile_file_status(&self.snapshot.workspace);
        let file_stem = profile_file_stem(role.label);
        let token_budget = self
            .snapshot
            .token_budget
            .map(|budget| format!("{budget} tokens"))
            .unwrap_or_else(|| "unbounded".to_string());

        let mut lines: Vec<Line> = Vec::new();
        let section = |lines: &mut Vec<Line>, label: &str, body: String| {
            lines.push(Line::from(Span::styled(
                label.to_string(),
                Style::default().fg(palette::DEEPSEEK_SKY).bold(),
            )));
            lines.push(Line::from(Span::styled(
                body,
                Style::default().fg(palette::TEXT_PRIMARY),
            )));
            lines.push(Line::from(""));
        };

        section(
            &mut lines,
            "Role",
            match self.roster_override_note() {
                Some(note) => format!("{} — {} · {note}", role.label, role.summary),
                None => format!("{} — {}", role.label, role.summary),
            },
        );
        section(
            &mut lines,
            "Model",
            format!(
                "{} (model_class_hint = {})  ·  route {} / {}, reasoning {}",
                model.label,
                self.selected_model_class(),
                self.snapshot.provider,
                self.snapshot.model,
                self.snapshot.reasoning
            ),
        );
        section(
            &mut lines,
            "Permissions",
            "Inherit the parent envelope and narrow only. Children cannot widen approval, trust, or secrets, and required approvals stay on.".to_string(),
        );
        section(
            &mut lines,
            "Tools",
            "Read tools by default; write tools for builders within scope; shell stays policy-gated; artifacts and receipts stay inspectable.".to_string(),
        );
        section(
            &mut lines,
            "Workspace & org",
            format!(
                "{} · sub-agents {} ({} concurrent, {} launch slots, {} admitted) · recursion agent {} / fleet {} (ceiling {})",
                self.snapshot.workspace.display(),
                if self.snapshot.subagents_enabled {
                    "enabled"
                } else {
                    "disabled"
                },
                self.snapshot.max_subagents,
                self.snapshot.launch_concurrency,
                self.snapshot.max_admitted,
                self.snapshot.subagent_spawn_depth,
                self.snapshot.fleet_spawn_depth,
                codewhale_config::MAX_SPAWN_DEPTH_CEILING,
            ),
        );
        section(
            &mut lines,
            "Review policy",
            format!(
                "Budget {token_budget} · {}s api, {}s heartbeat. Fleet -> exec runs the workers; /fleet status (or /subagents) inspects the ledger.",
                self.snapshot.api_timeout_secs, self.snapshot.heartbeat_timeout_secs
            ),
        );
        section(
            &mut lines,
            "Profile",
            format!(
                "{PROFILE_DIR}/{file_stem}.toml  ·  {profile_value} present. Start inserts a safe authoring prompt into the composer — nothing is written to disk.",
            ),
        );

        // `scroll` offsets by *visual* (post-wrap) rows, so the bound must count
        // wrapped rows — not logical lines — or the bottom sections become
        // unreachable. Estimate each line's wrapped height from its display
        // width; an over-estimate is harmless (scroll clamps at the real end).
        let wrap_width = usize::from(area.width).max(1);
        let visual_rows: usize = lines
            .iter()
            .map(|line| line.width().div_ceil(wrap_width).max(1))
            .sum();
        let max_scroll = visual_rows.saturating_sub(usize::from(area.height).max(1));
        let scroll = self.review_scroll.min(max_scroll);
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((scroll as u16, 0))
            .render(area, buf);
    }
}

/// Render a wizard choice step: a list of selectable identifiers on the left and
/// a wrapped detail pane (summary + description + context) on the right. Stacks
/// vertically when the body is too narrow for two columns so nothing truncates.
fn render_choice_step(
    area: Rect,
    buf: &mut Buffer,
    choices: &[Choice],
    selected: usize,
    context: &[String],
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (list_area, detail_area) = if area.width >= 56 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(22), Constraint::Min(20)])
            .split(area);
        (cols[0], cols[1])
    } else {
        let list_height = (choices.len() as u16 + 1).min(area.height.saturating_sub(1).max(1));
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(list_height), Constraint::Min(1)])
            .split(area);
        (rows[0], rows[1])
    };

    // List: labels are identifiers, so a `>`-marked single line each is safe.
    let list_width = usize::from(list_area.width);
    let mut list_lines: Vec<Line> = Vec::with_capacity(choices.len());
    for (idx, choice) in choices.iter().enumerate() {
        let is_selected = idx == selected;
        let pointer = if is_selected { "> " } else { "  " };
        let style = if is_selected {
            Style::default()
                .fg(palette::SELECTION_TEXT)
                .bg(palette::SELECTION_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette::TEXT_PRIMARY)
        };
        list_lines.push(Line::from(Span::styled(
            truncate_view_text(&format!("{pointer}{}", choice.label), list_width),
            style,
        )));
    }
    Paragraph::new(list_lines).render(list_area, buf);

    // Detail: summary + wrapped description + wrapped context, all word-wrapped.
    let choice = &choices[selected.min(choices.len().saturating_sub(1))];
    let mut detail_lines: Vec<Line> = vec![
        Line::from(Span::styled(
            choice.summary,
            Style::default().fg(palette::WHALE_ACCENT_PRIMARY).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            choice.description,
            Style::default().fg(palette::TEXT_PRIMARY),
        )),
    ];
    if !context.is_empty() {
        detail_lines.push(Line::from(""));
        for entry in context {
            detail_lines.push(Line::from(Span::styled(
                entry.clone(),
                Style::default().fg(palette::TEXT_MUTED),
            )));
        }
    }
    Paragraph::new(detail_lines)
        .wrap(Wrap { trim: true })
        .render(detail_area, buf);
}

fn profile_file_status(workspace: &Path) -> (String, String) {
    let dir = workspace.join(PROFILE_DIR);
    if !dir.exists() {
        return (
            "0 files".to_string(),
            format!("create {PROFILE_DIR}/*.toml"),
        );
    }
    if !dir.is_dir() {
        return (
            "blocked".to_string(),
            format!("{} is not a dir", dir.display()),
        );
    }

    let count = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("toml"))
        .count();

    if count == 1 {
        ("1 file".to_string(), PROFILE_DIR.to_string())
    } else {
        (format!("{count} files"), PROFILE_DIR.to_string())
    }
}

/// Map a Model-step row label to a profile-schema `model_class_hint` value.
/// Unknown/route-context labels resolve to `inherit`.
fn model_class_hint(label: &str) -> &'static str {
    match label {
        "fast" => "fast",
        "balanced" => "balanced",
        // "strong" = security/release/architecture → the strongest schema class.
        "strong" => "deep-reasoning",
        "deep-reasoning" => "deep-reasoning",
        "tool-heavy" => "tool-heavy",
        _ => "inherit",
    }
}

/// Sanitize a planner role label into a safe TOML file stem.
fn profile_file_stem(role: &str) -> String {
    let stem: String = role
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let stem = stem.trim_matches('-').to_ascii_lowercase();
    if stem.is_empty() {
        "custom".to_string()
    } else {
        stem
    }
}

fn profile_authoring_prompt(
    snapshot: &FleetSetupSnapshot,
    role: &str,
    model_class: &str,
) -> String {
    let file_stem = profile_file_stem(role);
    format!(
        "Create a safe CodeWhale Fleet agent profile file for this workspace.\n\n\
         Selected planner role: {role}. Selected model class: {model_class}.\n\
         Target path: {PROFILE_DIR}/{file_stem}.toml\n\
         Current route context only: provider = {provider}, model = {model}, reasoning = {reasoning}\n\n\
         Write TOML using only this schema:\n\
         - name\n\
         - display_name\n\
         - description\n\
         - role_hint (set to \"{role}\")\n\
         - model_class_hint (set to \"{model_class}\"; one of inherit, fast, balanced, deep-reasoning, code, review, or tool-heavy)\n\
         - model (optional explicit model id on the active/resolved route; omit to inherit the current route)\n\
         - [instructions].text\n\
         - [tools].posture = \"read-only\"\n\n\
         Do not include provider, base_url, api_key, auth, secrets, trust, allow_shell, or approval_required=false.\n\
         If model is present, keep it to a visible model id such as deepseek-v4-pro or glm-5.2.\n\
         Fleet product shape:\n\
         - Fleet is the durable sub-agent config surface: slots, profiles, models, tools, and ledger\n\
         - one main orchestrator profile coordinates the Fleet run and verifies returned claims\n\
         - workers are summoned as focused Fleet members with only their assigned slice\n\
         - default model behavior is same-route inheritance; choose fast/strong/code/review only when the role needs it\n\
         - DeepSeek-style model tiers are recommendations, not hierarchy rules; every slot may override model\n\
         - Workflow plans may select and monitor Fleet slots, but Fleet owns the worker config\n\
         - do not encode a recursive worker tree in [instructions].text; topology belongs to the orchestrator, not each worker\n\n\
         Keep the profile permission-narrowing and compatible with recursive Fleet role workers.",
        provider = snapshot.provider,
        model = snapshot.model,
        reasoning = snapshot.reasoning
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::views::ViewStack;
    use crossterm::event::KeyModifiers;
    use unicode_width::UnicodeWidthStr;

    const BLOCKER_SIZES: [(u16, u16); 4] = [(80, 24), (100, 30), (120, 32), (160, 40)];

    fn snapshot() -> FleetSetupSnapshot {
        FleetSetupSnapshot {
            workspace: PathBuf::from("/tmp/codewhale-test-workspace"),
            locale: crate::localization::Locale::En,
            provider_ready: true,
            provider: "DeepSeek".to_string(),
            model: "deepseek-v4-pro".to_string(),
            reasoning: "Auto".to_string(),
            subagents_enabled: true,
            max_subagents: 8,
            launch_concurrency: 3,
            max_admitted: 20,
            subagent_spawn_depth: 3,
            fleet_spawn_depth: 3,
            token_budget: Some(100_000),
            api_timeout_secs: 120,
            heartbeat_timeout_secs: 300,
            roster_members: crate::fleet::roster::FleetRoster::built_ins_only()
                .members()
                .iter()
                .map(|member| (member.id.to_lowercase(), member.origin.to_string()))
                .collect(),
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn sample_draft() -> Box<crate::fleet::profile::FleetProfileDraft> {
        let crate::fleet::profile::UntrustedProfileParse::Drafted(draft) =
            crate::fleet::profile::FleetProfileDraft::from_untrusted_json(
                r#"{"id":"reviewer","role_hint":"reviewer","description":"Reviews diffs.","instructions":"Read. Report. Stop."}"#,
            )
        else {
            panic!("sample draft should parse");
        };
        draft
    }

    fn to_review(view: &mut FleetSetupView) {
        view.handle_key(key(KeyCode::Enter));
        view.handle_key(key(KeyCode::Enter));
        assert_eq!(view.step, Step::Review);
    }

    #[test]
    fn review_step_m_requests_model_draft_with_current_answers() {
        let mut view = FleetSetupView::from_snapshot(snapshot());
        to_review(&mut view);

        let action = view.handle_key(key(KeyCode::Char('m')));
        let ViewAction::Emit(ViewEvent::FleetProfileModelDraftRequested {
            role,
            model_class,
            locale,
        }) = action
        else {
            panic!("expected model draft request");
        };
        assert!(!role.is_empty());
        assert!(!model_class.is_empty());
        assert_eq!(locale, crate::localization::Locale::En);
    }

    #[test]
    fn ratify_is_inert_without_a_draft_and_commits_with_one() {
        let mut view = FleetSetupView::from_snapshot(snapshot());
        to_review(&mut view);

        // No draft installed: g does nothing, m is the offered action.
        assert!(matches!(
            view.handle_key(key(KeyCode::Char('g'))),
            ViewAction::None
        ));

        let (title, content) = view.install_model_draft(sample_draft(), "GLM-5.2".to_string());
        assert!(title.contains("GLM-5.2"));
        assert!(content.contains("id = \"reviewer\""), "{content}");
        assert!(content.contains("Nothing is saved until"), "{content}");

        let action = view.handle_key(key(KeyCode::Char('g')));
        let ViewAction::EmitAndClose(ViewEvent::FleetProfileDraftCommitRequested { draft }) =
            action
        else {
            panic!("expected ratify commit event");
        };
        assert_eq!(draft.id, "reviewer");
    }

    #[test]
    fn changing_answers_discards_a_stale_draft() {
        let mut view = FleetSetupView::from_snapshot(snapshot());
        to_review(&mut view);
        let _ = view.install_model_draft(sample_draft(), "GLM-5.2".to_string());
        assert!(view.model_draft.is_some());

        // Back to the role step and change the selection: the draft no
        // longer matches the answers and must not survive to ratification.
        view.handle_key(key(KeyCode::Left));
        view.handle_key(key(KeyCode::Left));
        assert_eq!(view.step, Step::Role);
        view.handle_key(key(KeyCode::Down));
        assert!(view.model_draft.is_none());

        to_review(&mut view);
        assert!(matches!(
            view.handle_key(key(KeyCode::Char('g'))),
            ViewAction::None
        ));
    }

    #[test]
    fn arrows_move_within_step_and_enter_advances() {
        let mut view = FleetSetupView::from_snapshot(snapshot());
        assert_eq!(view.step, Step::Role);

        view.handle_key(key(KeyCode::Down));
        assert_eq!(view.role_idx, 1);

        view.handle_key(key(KeyCode::Enter));
        assert_eq!(view.step, Step::Model);

        view.handle_key(key(KeyCode::Down));
        assert_eq!(view.model_idx, 1);

        view.handle_key(key(KeyCode::Enter));
        assert_eq!(view.step, Step::Review);

        // Left steps back through the wizard.
        view.handle_key(key(KeyCode::Left));
        assert_eq!(view.step, Step::Model);
        view.handle_key(key(KeyCode::Left));
        assert_eq!(view.step, Step::Role);
    }

    #[test]
    fn esc_cancels_from_any_step() {
        let mut view = FleetSetupView::from_snapshot(snapshot());
        view.handle_key(key(KeyCode::Enter)); // -> Model
        let action = view.handle_key(key(KeyCode::Esc));
        assert!(matches!(action, ViewAction::Close));
    }

    #[test]
    fn start_on_review_inserts_profile_prompt_for_selection() {
        let mut view = FleetSetupView::from_snapshot(snapshot());
        // Role: manager(0) main(1) scout(2) builder(3) -> builder.
        view.handle_key(key(KeyCode::Down));
        view.handle_key(key(KeyCode::Down));
        view.handle_key(key(KeyCode::Down));
        view.handle_key(key(KeyCode::Enter)); // -> Model
        // Model: inherit(0) fast(1) -> fast.
        view.handle_key(key(KeyCode::Down));
        view.handle_key(key(KeyCode::Enter)); // -> Review

        let action = view.handle_key(key(KeyCode::Enter)); // Start
        match action {
            ViewAction::EmitAndClose(ViewEvent::CommandPaletteSelected {
                action: CommandPaletteAction::InsertText { text },
            }) => {
                assert!(text.contains("Target path: .codewhale/agents/builder.toml"));
                assert!(text.contains("role_hint (set to \"builder\")"));
                assert!(text.contains("model_class_hint (set to \"fast\""));
                assert!(text.contains("provider = DeepSeek"));
                assert!(text.contains("Do not include provider, base_url"));
                assert!(text.contains("Fleet is the durable sub-agent config surface"));
                assert!(text.contains("topology belongs to the orchestrator"));
            }
            other => panic!("expected profile prompt insertion, got {other:?}"),
        }
    }

    #[test]
    fn role_and_review_steps_note_roster_overrides() {
        // "reviewer" (index 4) collides with the built-in roster member; the
        // role step context and review Role section must both say so.
        let mut view = FleetSetupView::from_snapshot(snapshot());
        for _ in 0..4 {
            view.handle_key(key(KeyCode::Down));
        }
        assert_eq!(view.selected_role(), "reviewer");
        assert_eq!(
            view.roster_override_note().as_deref(),
            Some("Overrides the built-in 'reviewer' roster member.")
        );

        let role_step = render_through_stack(
            || {
                let mut v = FleetSetupView::from_snapshot(snapshot());
                for _ in 0..4 {
                    v.handle_key(key(KeyCode::Down));
                }
                v
            },
            120,
            40,
        )
        .join("\n");
        assert!(
            role_step.contains("Overrides the built-in 'reviewer'"),
            "{role_step}"
        );

        let review = render_through_stack(
            || {
                let mut v = FleetSetupView::from_snapshot(snapshot());
                for _ in 0..4 {
                    v.handle_key(key(KeyCode::Down));
                }
                v.step = Step::Review;
                v
            },
            120,
            40,
        )
        .join("\n");
        assert!(
            review.contains("Overrides the built-in 'reviewer'"),
            "{review}"
        );

        // "main" matches no roster member: no override note anywhere.
        let mut main_view = FleetSetupView::from_snapshot(snapshot());
        main_view.handle_key(key(KeyCode::Down));
        assert_eq!(main_view.selected_role(), "main");
        assert!(main_view.roster_override_note().is_none());
    }

    #[test]
    fn default_selection_targets_manager_inherit() {
        let view = FleetSetupView::from_snapshot(snapshot());
        let prompt = view.profile_prompt();
        assert!(prompt.contains("Target path: .codewhale/agents/manager.toml"));
        assert!(prompt.contains("model_class_hint (set to \"inherit\""));
        assert!(prompt.contains("Current route context only"));
        assert!(prompt.contains("permission-narrowing"));
    }

    fn render_through_stack(view_at: impl Fn() -> FleetSetupView, w: u16, h: u16) -> Vec<String> {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        for y in 0..h {
            for x in 0..w {
                buf[(x, y)].set_symbol("X");
            }
        }
        let mut stack = ViewStack::new();
        stack.push(view_at());
        stack.render(area, &mut buf);
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn fleet_setup_is_usable_and_opaque_at_blocker_sizes() {
        // Exercise each step so all three screens are validated at every size.
        type Builder = (&'static str, fn() -> FleetSetupView);
        let builders: [Builder; 3] = [
            ("role", || FleetSetupView::from_snapshot(snapshot())),
            ("model", || {
                let mut v = FleetSetupView::from_snapshot(snapshot());
                v.step = Step::Model;
                v
            }),
            ("review", || {
                let mut v = FleetSetupView::from_snapshot(snapshot());
                v.step = Step::Review;
                v
            }),
        ];

        for (label, make) in builders {
            for (w, h) in BLOCKER_SIZES {
                let rows = render_through_stack(make, w, h);
                let text = rows.join("\n");

                // No bleed-through anywhere in the composited frame.
                assert!(
                    !text.contains('X'),
                    "{label} {w}x{h}: background bleed-through"
                );
                // Some action label is always visible.
                assert!(text.contains("cancel"), "{label} {w}x{h}: missing footer");
                // The first impression communicates Fleet = agent team.
                assert!(
                    text.contains("agent team"),
                    "{label} {w}x{h}: missing framing"
                );
                // No row overflows the frame width.
                for (y, row) in rows.iter().enumerate() {
                    assert!(
                        UnicodeWidthStr::width(row.trim_end()) <= w as usize,
                        "{label} {w}x{h}: row {y} overflows: {row:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn review_lists_model_permissions_tools_and_scope() {
        // Top of the review: the leading sections are visible without scrolling.
        let top = render_through_stack(
            || {
                let mut v = FleetSetupView::from_snapshot(snapshot());
                v.step = Step::Review;
                v
            },
            120,
            40,
        )
        .join("\n");
        for section in ["Role", "Model", "Permissions", "Tools"] {
            assert!(top.contains(section), "review missing section: {section}");
        }

        // The review is intentionally scrollable; scrolling to the bottom reveals
        // the workspace/org scope, review policy, and the honest "no disk write"
        // note on the Start action.
        let bottom = render_through_stack(
            || {
                let mut v = FleetSetupView::from_snapshot(snapshot());
                v.step = Step::Review;
                v.review_scroll = 999; // clamps to max in render
                v
            },
            120,
            40,
        )
        .join("\n");
        for needle in ["Workspace", "Review policy", "nothing is written to disk"] {
            assert!(bottom.contains(needle), "scrolled review missing: {needle}");
        }
    }
}
