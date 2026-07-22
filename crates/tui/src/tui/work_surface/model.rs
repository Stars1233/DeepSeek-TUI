use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Component, Path};

use ratatui::layout::Rect;

use crate::tools::subagent::{AgentWorkerStatus, SubAgentStatus};
use crate::tui::app::{AgentCurrentActivityStatus, App, SidebarRowAction};
use crate::tui::history::{FileActivityKind, FileActivitySummary, HistoryCell};
use crate::work_graph::{
    AcceptanceRequirement, EdgeKind, EvidenceKind, EvidenceKindTag, NodeKind, NodeState,
    OperationBinding, OwnerState, Provenance, WorkGraphSnapshot, WorkNode,
};

/// Persisted Ocean work-surface placement. Bottom is deliberately absent: the
/// composer and phase footer own the shell's lower edge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WorkSurfacePlacement {
    #[default]
    Top,
    Left,
    Right,
}

impl WorkSurfacePlacement {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "left" => Self::Left,
            "right" => Self::Right,
            _ => Self::Top,
        }
    }

    #[must_use]
    pub const fn as_setting(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkRowId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkTone {
    Heading,
    Live,
    Attention,
    Success,
    Muted,
}

#[derive(Debug, Clone)]
pub(super) struct WorkRow {
    pub id: WorkRowId,
    pub mark: &'static str,
    pub label: String,
    pub detail: String,
    pub tone: WorkTone,
    pub selectable: bool,
    pub primary_action: Option<SidebarRowAction>,
}

#[derive(Debug, Clone)]
pub(super) struct WorkHitbox {
    pub id: WorkRowId,
    pub row_y: u16,
}

#[derive(Debug, Clone)]
enum WorkSourceState {
    Error(String),
    Disconnected,
}

impl WorkSourceState {
    const fn label(&self) -> &'static str {
        match self {
            Self::Error(_) => "error",
            Self::Disconnected => "disconnected",
        }
    }

    fn detail(&self) -> &str {
        match self {
            Self::Error(error) => error,
            Self::Disconnected => "Work Graph runtime is not attached",
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkSurfaceState {
    pub placement: WorkSurfacePlacement,
    pub(super) effective_placement: WorkSurfacePlacement,
    /// Focus owner axis — distinct from selection and detail-open.
    pub focused: bool,
    /// Keyboard/mouse selection highlight.
    pub selected: Option<WorkRowId>,
    /// Which row currently owns an open detail (pager / agent card).
    pub opened: Option<WorkRowId>,
    pub scroll_offset: usize,
    pub last_area: Option<Rect>,
    pub visible_rows: usize,
    pub total_rows: usize,
    pub(super) hovered: Option<WorkRowId>,
    pub(super) hitboxes: Vec<WorkHitbox>,
    pub(super) cached_graph: Option<WorkGraphSnapshot>,
    pub(super) latest_rows: Vec<WorkRow>,
}

impl Default for WorkSurfaceState {
    fn default() -> Self {
        Self::with_placement(WorkSurfacePlacement::Top)
    }
}

impl WorkSurfaceState {
    #[must_use]
    pub fn with_placement(placement: WorkSurfacePlacement) -> Self {
        Self {
            placement,
            effective_placement: placement,
            focused: false,
            selected: None,
            opened: None,
            scroll_offset: 0,
            last_area: None,
            visible_rows: 0,
            total_rows: 0,
            hovered: None,
            hitboxes: Vec::new(),
            cached_graph: None,
            latest_rows: Vec::new(),
        }
    }

    pub(super) fn selected_index(&self, rows: &[WorkRow]) -> Option<usize> {
        self.selected
            .as_ref()
            .and_then(|selected| rows.iter().position(|row| &row.id == selected))
    }

    /// Keep row identity and the viewport offset valid without moving the
    /// viewport to the remembered keyboard selection. Mouse-wheel scrolling
    /// is allowed to leave that selection off-screen until keyboard
    /// navigation resumes.
    pub(super) fn clamp_viewport(&mut self, rows: &[WorkRow]) {
        let selectable = rows.iter().filter(|row| row.selectable).collect::<Vec<_>>();
        if selectable.is_empty() {
            self.selected = None;
            self.focused = false;
            self.scroll_offset = 0;
            return;
        }
        if !selectable
            .iter()
            .any(|row| Some(&row.id) == self.selected.as_ref())
        {
            self.selected = Some(selectable[0].id.clone());
        }
        self.scroll_offset = self
            .scroll_offset
            .min(rows.len().saturating_sub(self.visible_rows.max(1)));
    }

    /// Reveal the remembered selection after keyboard navigation. Rendering
    /// alone must use `clamp_viewport`; otherwise every redraw undoes a mouse
    /// wheel offset when the selection is above the viewport.
    pub(super) fn clamp_selection(&mut self, rows: &[WorkRow]) {
        self.clamp_viewport(rows);
        let Some(selected) = self.selected_index(rows) else {
            return;
        };
        if selected < self.scroll_offset {
            self.scroll_offset = selected;
        } else if self.visible_rows > 0
            && selected >= self.scroll_offset.saturating_add(self.visible_rows)
        {
            self.scroll_offset = selected.saturating_add(1).saturating_sub(self.visible_rows);
        }
        self.scroll_offset = self
            .scroll_offset
            .min(rows.len().saturating_sub(self.visible_rows.max(1)));
    }
}

pub(super) fn project(app: &mut App) -> Vec<WorkRow> {
    let active_session = app.current_session_id.is_some();
    let agents = agent_rows(app);
    let coordination = coordination_row(app);
    let activity = settled_file_activity(app);
    let capture = app.runtime_services.work.as_ref().map(|work| {
        work.try_capture(app.current_session_id.as_deref())
            .map(|snapshot| snapshot.map(|snapshot| snapshot.graph))
    });

    let (graph, source_state) = match capture {
        Some(Ok(Some(graph))) => {
            app.work_surface.cached_graph = Some(graph.clone());
            (Some(graph), None)
        }
        Some(Ok(None)) => {
            app.work_surface.cached_graph = None;
            (None, None)
        }
        Some(Err(error)) => (
            app.work_surface.cached_graph.clone(),
            active_session.then_some(WorkSourceState::Error(error)),
        ),
        None => (
            app.work_surface.cached_graph.clone(),
            active_session.then_some(WorkSourceState::Disconnected),
        ),
    };

    let rows = match graph {
        Some(graph) => graph_rows(
            &graph,
            source_state.as_ref(),
            agents,
            coordination,
            activity,
        ),
        None if !agents.is_empty() || coordination.is_some() || !activity.is_empty() => {
            ordered_rows(None, source_state.as_ref(), agents, coordination, activity)
        }
        None => source_state.map_or_else(Vec::new, |state| {
            vec![section_heading(
                "work",
                &format!("Work · {}", state.label()),
                state.detail(),
            )]
        }),
    };
    app.work_surface.latest_rows = rows.clone();
    if let Some(opened) = app.work_surface.opened.as_ref()
        && !rows.iter().any(|row| &row.id == opened)
    {
        app.work_surface.opened = None;
    }
    rows
}

fn graph_rows(
    snapshot: &WorkGraphSnapshot,
    source_state: Option<&WorkSourceState>,
    agents: Vec<RankedWorkRow>,
    coordination: Option<RankedWorkRow>,
    activity: SettledFileActivity,
) -> Vec<WorkRow> {
    ordered_rows(Some(snapshot), source_state, agents, coordination, activity)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkBucket {
    Active,
    Attention,
    Ready,
    Recent,
}

impl WorkBucket {
    const fn rank(self) -> u8 {
        match self {
            Self::Active => 0,
            Self::Attention => 1,
            Self::Ready => 2,
            Self::Recent => 3,
        }
    }
}

struct RankedWorkRow {
    bucket: WorkBucket,
    order: usize,
    row: WorkRow,
}

#[derive(Default)]
struct SettledFileActivity {
    summary: FileActivitySummary,
    read: Vec<String>,
    list: Vec<String>,
    search: Vec<String>,
    write: Vec<String>,
}

impl SettledFileActivity {
    fn is_empty(&self) -> bool {
        self.summary.is_empty()
    }
}

fn ordered_rows(
    snapshot: Option<&WorkGraphSnapshot>,
    source_state: Option<&WorkSourceState>,
    mut ranked: Vec<RankedWorkRow>,
    coordination: Option<RankedWorkRow>,
    activity: SettledFileActivity,
) -> Vec<WorkRow> {
    ranked.extend(coordination);
    if let Some(snapshot) = snapshot {
        ranked.extend(
            snapshot
                .nodes
                .iter()
                .filter(|node| {
                    matches!(
                        node.kind,
                        NodeKind::PlanStep | NodeKind::Operation | NodeKind::Blocker
                    )
                })
                .filter(|node| !is_settled_transient_operation(node))
                .enumerate()
                .map(|(order, node)| RankedWorkRow {
                    bucket: node_bucket(node),
                    order: 10_000usize.saturating_add(order),
                    row: graph_node_row(snapshot, node),
                }),
        );
    }
    ranked.extend(activity_rows(activity));
    ranked.sort_by_key(|item| (item.bucket.rank(), item.order));

    let active = ranked
        .iter()
        .filter(|item| item.bucket == WorkBucket::Active)
        .count();
    let attention = ranked
        .iter()
        .filter(|item| item.bucket == WorkBucket::Attention)
        .count();
    let ready = ranked
        .iter()
        .filter(|item| item.bucket == WorkBucket::Ready)
        .count();
    let recent = ranked
        .iter()
        .filter(|item| item.bucket == WorkBucket::Recent)
        .count();
    let source = source_state
        .map(|state| format!(" · {}", state.label()))
        .unwrap_or_default();
    let detail = match (snapshot, source_state) {
        (Some(snapshot), Some(state)) => {
            format!("graph revision {} · {}", snapshot.revision, state.detail())
        }
        (Some(snapshot), None) => format!("graph revision {}", snapshot.revision),
        (None, Some(state)) => state.detail().to_string(),
        (None, None) => "Current session activity".to_string(),
    };
    let mut rows = vec![section_heading(
        "work",
        &format!(
            "Work · {active} active · {attention} needs input · {ready} ready · {recent} recent{source}"
        ),
        &detail,
    )];
    rows.extend(ranked.into_iter().map(|item| item.row));
    rows
}

fn coordination_row(app: &App) -> Option<RankedWorkRow> {
    let projection = app.coordination_detail.as_ref()?;
    if projection.decisions.is_empty()
        && projection.write_claims.is_empty()
        && projection.reconciliations.is_empty()
        && projection.context_projections.is_empty()
        && projection.contentions.is_empty()
    {
        return None;
    }
    let attention = crate::tui::coordination_detail::needs_attention(projection);
    let bucket = if attention {
        WorkBucket::Attention
    } else {
        WorkBucket::Recent
    };
    Some(RankedWorkRow {
        bucket,
        // Coordination is a session-wide receipt, before individual workers
        // within the same bucket but after live/attention priority sorting.
        order: 100,
        row: WorkRow {
            id: WorkRowId("coordination".to_string()),
            mark: if attention {
                crate::tui::glyphs::ATTENTION
            } else {
                crate::tui::glyphs::DONE
            },
            label: "Coordination Work".to_string(),
            detail: crate::tui::coordination_detail::summary(projection),
            tone: bucket_tone(bucket),
            selectable: true,
            primary_action: Some(SidebarRowAction::InspectWork {
                title: "Coordination Work".to_string(),
                body: crate::tui::coordination_detail::format(projection),
                stop_action: None,
            }),
        },
    })
}

fn node_bucket(node: &WorkNode) -> WorkBucket {
    match node.state {
        NodeState::Initializing | NodeState::Active => WorkBucket::Active,
        NodeState::Waiting | NodeState::Blocked | NodeState::Stale | NodeState::Failed => {
            WorkBucket::Attention
        }
        NodeState::Completed if !node.acceptance.is_empty() => WorkBucket::Attention,
        NodeState::Ready => WorkBucket::Ready,
        NodeState::Completed
        | NodeState::Verified
        | NodeState::Superseded
        | NodeState::Cancelled => WorkBucket::Recent,
    }
}

fn agent_rows(app: &App) -> Vec<RankedWorkRow> {
    let cached_ids = app
        .subagent_cache
        .iter()
        .filter(|agent| !agent.from_prior_session)
        .map(|agent| agent.agent_id.as_str())
        .collect::<HashSet<_>>();
    let mut rows = app
        .subagent_cache
        .iter()
        .filter(|agent| !agent.from_prior_session)
        .enumerate()
        .map(|(order, agent)| {
            let meta = app.agent_progress_meta.get(&agent.agent_id);
            let current_activity = meta.and_then(|meta| meta.current_activity.as_ref());
            let status = current_activity
                .map(|activity| current_activity_status_label(activity.status))
                .or_else(|| agent.worker_status.map(worker_status_label))
                .unwrap_or_else(|| subagent_status_label(&agent.status));
            let bucket = current_activity
                .map(|activity| current_activity_status_bucket(activity.status))
                .or_else(|| agent.worker_status.map(worker_status_bucket))
                .unwrap_or_else(|| subagent_status_bucket(&agent.status));
            let role = agent
                .assignment
                .role
                .as_deref()
                .filter(|role| !role.trim().is_empty())
                .unwrap_or_else(|| agent.agent_type.as_str());
            let name = app
                .agent_label_map
                .get(&agent.agent_id)
                .cloned()
                .or_else(|| agent.nickname.clone())
                .unwrap_or_else(|| agent.name.clone());
            let mut facts = vec![
                status.to_string(),
                summarize_assignment(&agent.assignment.objective),
            ];
            if let Some(detail) = current_activity.and_then(|activity| activity.detail.as_deref()) {
                facts.push(detail.to_string());
            }
            if let Some(tool) =
                current_activity.and_then(|activity| activity.current_tool.as_deref())
            {
                facts.push(format!("using {tool}"));
            }
            if let Some(step) = current_activity.and_then(|activity| activity.step) {
                facts.push(format!("step {step}"));
            }
            if let Some(files) = meta
                .map(|meta| meta.files_touched)
                .filter(|count| *count > 0)
            {
                facts.push(format!("{files} files changed"));
            }
            RankedWorkRow {
                bucket,
                order,
                row: WorkRow {
                    id: WorkRowId(format!("worker:{}", agent.agent_id)),
                    mark: agent_mark(bucket),
                    label: format!("Agent {name} · {role}"),
                    detail: facts.join(" · "),
                    tone: bucket_tone(bucket),
                    selectable: true,
                    primary_action: Some(SidebarRowAction::OpenAgentDetail {
                        agent_id: agent.agent_id.clone(),
                    }),
                },
            }
        })
        .collect::<Vec<_>>();

    let mut progress_only = app
        .agent_progress
        .iter()
        .filter(|(id, _)| !cached_ids.contains(id.as_str()))
        .collect::<Vec<_>>();
    progress_only.sort_by_key(|(id, _)| (*id).clone());
    rows.extend(
        progress_only
            .into_iter()
            .enumerate()
            .map(|(order, (id, _progress))| {
                let meta = app.agent_progress_meta.get(id);
                let current_activity = meta.and_then(|meta| meta.current_activity.as_ref());
                let status = current_activity
                    .map(|activity| current_activity_status_label(activity.status))
                    .unwrap_or("running");
                let bucket = current_activity
                    .map(|activity| current_activity_status_bucket(activity.status))
                    .unwrap_or(WorkBucket::Active);
                let name = app
                    .agent_label_map
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.clone());
                let mut facts = vec![status.to_string()];
                if let Some(detail) =
                    current_activity.and_then(|activity| activity.detail.as_deref())
                {
                    facts.push(detail.to_string());
                }
                if let Some(tool) =
                    current_activity.and_then(|activity| activity.current_tool.as_deref())
                {
                    facts.push(format!("using {tool}"));
                }
                if let Some(step) = current_activity.and_then(|activity| activity.step) {
                    facts.push(format!("step {step}"));
                }
                if let Some(files) = meta
                    .map(|meta| meta.files_touched)
                    .filter(|count| *count > 0)
                {
                    facts.push(format!("{files} files changed"));
                }
                RankedWorkRow {
                    bucket,
                    order: 5_000usize.saturating_add(order),
                    row: WorkRow {
                        id: WorkRowId(format!("worker:{id}")),
                        mark: agent_mark(bucket),
                        label: format!("Agent {name}"),
                        detail: facts.join(" · "),
                        tone: bucket_tone(bucket),
                        selectable: true,
                        primary_action: Some(SidebarRowAction::OpenAgentDetail {
                            agent_id: id.clone(),
                        }),
                    },
                }
            }),
    );
    rows
}

fn summarize_assignment(value: &str) -> String {
    crate::tui::history::summarize_tool_output(value)
}

fn current_activity_status_bucket(status: AgentCurrentActivityStatus) -> WorkBucket {
    match status {
        AgentCurrentActivityStatus::Waiting
        | AgentCurrentActivityStatus::Interrupted
        | AgentCurrentActivityStatus::Failed => WorkBucket::Attention,
        AgentCurrentActivityStatus::Queued => WorkBucket::Ready,
        AgentCurrentActivityStatus::Done | AgentCurrentActivityStatus::Canceled => {
            WorkBucket::Recent
        }
        AgentCurrentActivityStatus::Starting
        | AgentCurrentActivityStatus::Running
        | AgentCurrentActivityStatus::ModelWait
        | AgentCurrentActivityStatus::RunningTool => WorkBucket::Active,
    }
}

fn current_activity_status_label(status: AgentCurrentActivityStatus) -> &'static str {
    match status {
        AgentCurrentActivityStatus::Queued => "queued",
        AgentCurrentActivityStatus::Starting => "starting",
        AgentCurrentActivityStatus::Running => "running",
        AgentCurrentActivityStatus::ModelWait => "waiting for model",
        AgentCurrentActivityStatus::RunningTool => "running tool",
        AgentCurrentActivityStatus::Waiting => "waiting for input",
        AgentCurrentActivityStatus::Done => "completed",
        AgentCurrentActivityStatus::Failed => "failed",
        AgentCurrentActivityStatus::Canceled => "cancelled",
        AgentCurrentActivityStatus::Interrupted => "interrupted",
    }
}

fn worker_status_bucket(status: AgentWorkerStatus) -> WorkBucket {
    match status {
        AgentWorkerStatus::WaitingForUser
        | AgentWorkerStatus::Interrupted
        | AgentWorkerStatus::Failed => WorkBucket::Attention,
        AgentWorkerStatus::Queued => WorkBucket::Ready,
        AgentWorkerStatus::Completed | AgentWorkerStatus::Cancelled => WorkBucket::Recent,
        AgentWorkerStatus::Starting
        | AgentWorkerStatus::Running
        | AgentWorkerStatus::ModelWait
        | AgentWorkerStatus::RunningTool => WorkBucket::Active,
    }
}

fn worker_status_label(status: AgentWorkerStatus) -> &'static str {
    match status {
        AgentWorkerStatus::Queued => "queued",
        AgentWorkerStatus::Starting => "starting",
        AgentWorkerStatus::Running => "running",
        AgentWorkerStatus::WaitingForUser => "waiting for input",
        AgentWorkerStatus::ModelWait => "waiting for model",
        AgentWorkerStatus::RunningTool => "running tool",
        AgentWorkerStatus::Completed => "completed",
        AgentWorkerStatus::Failed => "failed",
        AgentWorkerStatus::Cancelled => "cancelled",
        AgentWorkerStatus::Interrupted => "interrupted",
    }
}

fn subagent_status_bucket(status: &SubAgentStatus) -> WorkBucket {
    match status {
        SubAgentStatus::Running => WorkBucket::Active,
        SubAgentStatus::Interrupted(_)
        | SubAgentStatus::Failed(_)
        | SubAgentStatus::BudgetExhausted => WorkBucket::Attention,
        SubAgentStatus::Completed | SubAgentStatus::Cancelled => WorkBucket::Recent,
    }
}

fn subagent_status_label(status: &SubAgentStatus) -> &'static str {
    match status {
        SubAgentStatus::Running => "running",
        SubAgentStatus::Completed => "completed",
        SubAgentStatus::Interrupted(_) => "interrupted",
        SubAgentStatus::Failed(_) => "failed",
        SubAgentStatus::Cancelled => "cancelled",
        SubAgentStatus::BudgetExhausted => "budget exhausted",
    }
}

const fn bucket_tone(bucket: WorkBucket) -> WorkTone {
    match bucket {
        WorkBucket::Active => WorkTone::Live,
        WorkBucket::Attention => WorkTone::Attention,
        WorkBucket::Ready => WorkTone::Muted,
        WorkBucket::Recent => WorkTone::Success,
    }
}

const fn agent_mark(bucket: WorkBucket) -> &'static str {
    match bucket {
        WorkBucket::Active => crate::tui::glyphs::SELECTION,
        WorkBucket::Attention => crate::tui::glyphs::ATTENTION,
        WorkBucket::Ready => crate::tui::glyphs::READY,
        WorkBucket::Recent => crate::tui::glyphs::DONE,
    }
}

fn settled_file_activity(app: &App) -> SettledFileActivity {
    let mut activity = SettledFileActivity::default();
    let mut seen = HashSet::new();
    for index in 0..app.virtual_cell_count() {
        let Some(HistoryCell::Tool(cell)) = app.cell_at_virtual_index(index) else {
            continue;
        };
        if !cell.is_success() {
            continue;
        }
        let Some(detail) = app.tool_detail_record_for_cell(index) else {
            continue;
        };
        let Some(kind) = FileActivitySummary::from_tool_name(&detail.tool_name) else {
            continue;
        };
        if !seen.insert(detail.tool_id.as_str()) {
            continue;
        }
        activity.summary.record(kind);
        let target = activity_target(&app.workspace, &detail.tool_name, &detail.input, kind);
        let details = match kind {
            FileActivityKind::Read => &mut activity.read,
            FileActivityKind::List => &mut activity.list,
            FileActivityKind::Search => &mut activity.search,
            FileActivityKind::Write => &mut activity.write,
        };
        if let Some(target) = target
            && details.len() < 12
            && !details.contains(&target)
        {
            details.push(target);
        }
    }
    activity
}

fn activity_rows(activity: SettledFileActivity) -> Vec<RankedWorkRow> {
    let summaries = activity.summary.compact_display();
    let details = [
        activity.read,
        activity.list,
        activity.search,
        activity.write,
    ];
    summaries
        .into_iter()
        .zip(details)
        .enumerate()
        .map(|(order, (label, details))| {
            let body = if details.is_empty() {
                "No safe target detail retained".to_string()
            } else {
                details.join("\n")
            };
            RankedWorkRow {
                bucket: WorkBucket::Recent,
                order: 20_000usize.saturating_add(order),
                row: WorkRow {
                    id: WorkRowId(format!("activity:{order}")),
                    mark: crate::tui::glyphs::DONE,
                    label: label.clone(),
                    detail: details
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "settled".to_string()),
                    tone: WorkTone::Success,
                    selectable: true,
                    primary_action: Some(SidebarRowAction::InspectWork {
                        title: format!("Work · {label}"),
                        body,
                        stop_action: None,
                    }),
                },
            }
        })
        .collect()
}

fn activity_target(
    workspace: &Path,
    tool_name: &str,
    input: &serde_json::Value,
    kind: FileActivityKind,
) -> Option<String> {
    if tool_name == "apply_patch"
        && let Ok(preflight) = crate::tools::apply_patch::preflight_apply_patch(input)
    {
        let targets = preflight
            .touched_files
            .iter()
            .filter_map(|path| privacy_safe_path(workspace, path))
            .take(4)
            .collect::<Vec<_>>();
        if !targets.is_empty() {
            return Some(targets.join(", "));
        }
    }
    let keys: &[&str] = match kind {
        FileActivityKind::Search => &["pattern", "query", "path"],
        _ => &["path", "file_path"],
    };
    keys.iter().find_map(|key| {
        let value = input.get(*key)?.as_str()?.trim();
        if value.is_empty() {
            return None;
        }
        if kind == FileActivityKind::Search && *key != "path" {
            return Some(safe_pattern(value));
        }
        privacy_safe_path(workspace, value)
    })
}

fn privacy_safe_path(workspace: &Path, raw: &str) -> Option<String> {
    let path = Path::new(raw);
    let normalized_raw = raw.replace('\\', "/");
    let normalized_workspace = workspace.to_string_lossy().replace('\\', "/");
    let relative = if path.is_absolute() || normalized_raw.starts_with('/') {
        let workspace_prefix = normalized_workspace.trim_end_matches('/');
        if normalized_raw == workspace_prefix {
            ""
        } else {
            normalized_raw.strip_prefix(&format!("{workspace_prefix}/"))?
        }
    } else {
        normalized_raw.as_str()
    };
    let relative = Path::new(relative);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    let display = relative.to_string_lossy().replace('\\', "/");
    (!display.is_empty()).then_some(display)
}

fn safe_pattern(raw: &str) -> String {
    let single_line = raw.replace(['\n', '\r', '\t'], " ");
    let mut chars = single_line.chars();
    let prefix = chars.by_ref().take(80).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn is_settled_transient_operation(node: &WorkNode) -> bool {
    node.kind == NodeKind::Operation
        && node
            .binding
            .as_ref()
            .is_some_and(|binding| !binding.durable)
        && match node.state {
            NodeState::Completed => node.acceptance.is_empty(),
            NodeState::Verified | NodeState::Superseded | NodeState::Cancelled => true,
            _ => false,
        }
}

fn section_heading(id: &str, label: &str, detail: &str) -> WorkRow {
    WorkRow {
        id: WorkRowId(format!("section:{id}")),
        mark: "▾",
        label: label.to_string(),
        detail: detail.to_string(),
        tone: WorkTone::Heading,
        selectable: false,
        primary_action: None,
    }
}

fn graph_node_row(snapshot: &WorkGraphSnapshot, node: &WorkNode) -> WorkRow {
    let (mark, tone) = match node.state {
        NodeState::Ready => (crate::tui::glyphs::READY, WorkTone::Muted),
        NodeState::Initializing => (crate::tui::glyphs::SELECTION, WorkTone::Live),
        NodeState::Active => (crate::tui::glyphs::SELECTION, WorkTone::Live),
        NodeState::Waiting => (crate::tui::glyphs::ATTENTION, WorkTone::Attention),
        NodeState::Blocked => ("!", WorkTone::Attention),
        NodeState::Completed if node.acceptance.is_empty() => {
            (crate::tui::glyphs::DONE, WorkTone::Success)
        }
        NodeState::Completed => ("!", WorkTone::Attention),
        NodeState::Verified => (crate::tui::glyphs::DONE, WorkTone::Success),
        NodeState::Stale => ("?", WorkTone::Attention),
        NodeState::Superseded | NodeState::Cancelled => ("−", WorkTone::Muted),
        NodeState::Failed => (crate::tui::glyphs::FAILED, WorkTone::Attention),
    };
    let state = state_label(node);
    let kind = kind_label(node.kind);
    let detail = if node.state == NodeState::Ready && node.kind == NodeKind::PlanStep {
        kind.to_string()
    } else {
        format!("{state} · {kind}")
    };
    let stop_action = node
        .state
        .is_live()
        .then(|| stop_action(node.binding.as_ref()))
        .flatten();
    WorkRow {
        id: WorkRowId(format!("graph:{}", node.id.as_str())),
        mark,
        label: node.title.clone(),
        detail,
        tone,
        selectable: true,
        primary_action: Some(SidebarRowAction::InspectWork {
            title: format!("Work · {}", node.title),
            body: inspector_text(snapshot, node),
            stop_action: stop_action.map(Box::new),
        }),
    }
}

fn state_label(node: &WorkNode) -> &'static str {
    match node.state {
        NodeState::Ready => "ready",
        NodeState::Initializing => "initializing",
        NodeState::Active => "running",
        NodeState::Waiting => "waiting",
        NodeState::Blocked => "blocked",
        NodeState::Completed if node.acceptance.is_empty() => "completed",
        NodeState::Completed => "completed · evidence pending",
        NodeState::Verified => "verified",
        NodeState::Stale => "stale",
        NodeState::Superseded => "superseded",
        NodeState::Cancelled => "cancelled",
        NodeState::Failed => "failed",
    }
}

const fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Objective => "objective",
        NodeKind::PlanStep => "plan step",
        NodeKind::Operation => "operation",
        NodeKind::Evidence => "evidence",
        NodeKind::Blocker => "blocker",
        NodeKind::Approval => "approval",
        NodeKind::RuntimeRef => "runtime",
        NodeKind::LaneRef => "lane",
    }
}

fn stop_action(binding: Option<&OperationBinding>) -> Option<SidebarRowAction> {
    let binding = binding?;
    if let Some(id) = binding.external.strip_prefix("task:") {
        Some(SidebarRowAction::Command(format!("/task cancel {id}")))
    } else if let Some(id) = binding.external.strip_prefix("shell:") {
        Some(SidebarRowAction::Command(format!("/jobs cancel {id}")))
    } else if let Some(id) = binding.external.strip_prefix("worker:") {
        Some(SidebarRowAction::CancelAgent {
            agent_id: id.to_string(),
        })
    } else {
        binding
            .external
            .strip_prefix("workflow:")
            .map(|id| SidebarRowAction::Command(format!("/workflow cancel {id}")))
    }
}

fn inspector_text(snapshot: &WorkGraphSnapshot, node: &WorkNode) -> String {
    let mut out = String::new();
    section_text(
        &mut out,
        "Objective",
        objective_for(snapshot, node)
            .as_deref()
            .unwrap_or("Not connected"),
    );
    section_list(
        &mut out,
        "Prerequisites",
        related_nodes(snapshot, node, EdgeKind::DependsOn, true),
    );
    section_text(
        &mut out,
        "Current",
        &format!("{} · {}", state_label(node), kind_label(node.kind)),
    );
    section_list(
        &mut out,
        "Downstream impact",
        related_nodes(snapshot, node, EdgeKind::DependsOn, false),
    );
    section_text(&mut out, "Binding + lifecycle owner", &binding_text(node));
    section_text(
        &mut out,
        "Evidence vs acceptance",
        &evidence_text(snapshot, node),
    );
    section_text(
        &mut out,
        "Blockers / approvals",
        &blockers_approvals_text(snapshot, node),
    );
    section_text(&mut out, "Why next", &why_next(snapshot, node));
    section_text(
        &mut out,
        "Provenance + last reconcile",
        &provenance_text(node),
    );
    if node.state == NodeState::Stale {
        section_text(
            &mut out,
            "Last bounded output",
            last_output_ref(snapshot, node)
                .as_deref()
                .unwrap_or("No output receipt"),
        );
    }
    out.trim_end().to_string()
}

fn objective_for(snapshot: &WorkGraphSnapshot, node: &WorkNode) -> Option<String> {
    if node.kind == NodeKind::Objective {
        return Some(node.title.clone());
    }
    let mut current = node.id.clone();
    let mut seen = HashSet::new();
    while seen.insert(current.clone()) {
        let Some(parent) = snapshot.edges.iter().find_map(|edge| {
            (edge.kind == EdgeKind::Contains && edge.to == current).then(|| edge.from.clone())
        }) else {
            break;
        };
        let Some(parent_node) = snapshot.node(&parent) else {
            break;
        };
        if parent_node.kind == NodeKind::Objective {
            return Some(parent_node.title.clone());
        }
        current = parent;
    }
    snapshot.compat.plan.objective.clone()
}

fn related_nodes(
    snapshot: &WorkGraphSnapshot,
    node: &WorkNode,
    kind: EdgeKind,
    outgoing: bool,
) -> Vec<String> {
    snapshot
        .edges
        .iter()
        .filter(|edge| edge.kind == kind)
        .filter_map(|edge| {
            let related = if outgoing && edge.from == node.id {
                Some(&edge.to)
            } else if !outgoing && edge.to == node.id {
                Some(&edge.from)
            } else {
                None
            }?;
            snapshot
                .node(related)
                .map(|related| format!("{} · {}", related.title, state_label(related)))
        })
        .collect()
}

fn binding_text(node: &WorkNode) -> String {
    let Some(binding) = node.binding.as_ref() else {
        return "Not bound".to_string();
    };
    let mut text = format!(
        "Owner: {}\nDurable: {}",
        binding.external,
        if binding.durable { "yes" } else { "no" }
    );
    if let Some(observation) = binding.last_observation.as_ref() {
        let owner_state = match observation.owner_state {
            OwnerState::Initializing => "initializing",
            OwnerState::Running => "running",
            OwnerState::Waiting => "waiting",
            OwnerState::Completed => "completed",
            OwnerState::Failed => "failed",
            OwnerState::Cancelled => "cancelled",
        };
        let _ = write!(
            text,
            "\nLast owner state: {owner_state}\nLast reconcile: {} ms UTC · sequence {}",
            observation.observed_at, observation.seq
        );
    } else {
        text.push_str("\nLast reconcile: never");
    }
    text
}

fn evidence_text(snapshot: &WorkGraphSnapshot, node: &WorkNode) -> String {
    let acceptance = if node.acceptance.is_empty() {
        vec!["- No evidence requirement".to_string()]
    } else {
        node.acceptance
            .iter()
            .map(|requirement| format!("- {}", acceptance_label(requirement)))
            .collect()
    };
    let evidence = evidence_for(snapshot, node);
    let evidence = if evidence.is_empty() {
        vec!["- None attached".to_string()]
    } else {
        evidence
            .into_iter()
            .map(|evidence| {
                let reference = evidence
                    .evidence
                    .as_ref()
                    .map(|item| item.reference())
                    .unwrap_or("invalid evidence node");
                format!("- {reference} · {}", state_label(evidence))
            })
            .collect()
    };
    format!(
        "Acceptance:\n{}\nEvidence:\n{}",
        acceptance.join("\n"),
        evidence.join("\n")
    )
}

fn acceptance_label(requirement: &AcceptanceRequirement) -> String {
    match requirement {
        AcceptanceRequirement::EvidenceOfKind { kind } => {
            let kind = match kind {
                EvidenceKindTag::ToolRun => "tool run",
                EvidenceKindTag::Artifact => "artifact",
                EvidenceKindTag::TestSummary => "test summary",
                EvidenceKindTag::Receipt => "receipt",
                EvidenceKindTag::Approval => "approval",
                EvidenceKindTag::Route => "route",
                EvidenceKindTag::WebCitation => "web citation",
            };
            format!("evidence of kind {kind}")
        }
    }
}

fn evidence_for<'a>(snapshot: &'a WorkGraphSnapshot, node: &WorkNode) -> Vec<&'a WorkNode> {
    snapshot
        .edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Verifies && edge.to == node.id)
        .filter_map(|edge| snapshot.node(&edge.from))
        .collect()
}

fn blockers_approvals_text(snapshot: &WorkGraphSnapshot, node: &WorkNode) -> String {
    let mut lines = Vec::new();
    lines.extend(
        related_nodes(snapshot, node, EdgeKind::Blocks, false)
            .into_iter()
            .map(|item| format!("- Blocked by {item}")),
    );
    lines.extend(
        related_nodes(snapshot, node, EdgeKind::RequiresApproval, true)
            .into_iter()
            .map(|item| format!("- Approval {item}")),
    );
    if node.kind == NodeKind::PlanStep {
        lines.extend(
            snapshot
                .nodes
                .iter()
                .filter(|candidate| candidate.kind == NodeKind::Approval)
                .map(|approval| format!("- {} · {}", approval.title, state_label(approval))),
        );
    }
    if lines.is_empty() {
        "None".to_string()
    } else {
        lines.join("\n")
    }
}

fn why_next(snapshot: &WorkGraphSnapshot, node: &WorkNode) -> String {
    match node.state {
        NodeState::Ready => {
            let pending = related_nodes(snapshot, node, EdgeKind::DependsOn, true);
            if pending.is_empty() {
                "Ready with no recorded prerequisite".to_string()
            } else {
                format!("Ready after: {}", pending.join(", "))
            }
        }
        NodeState::Initializing => "Spawn intent is registered; awaiting owner handle".to_string(),
        NodeState::Active => "Lifecycle owner reports active work".to_string(),
        NodeState::Waiting => "Waiting on an owner or approval".to_string(),
        NodeState::Blocked => "Blocked; resolve the causes above".to_string(),
        NodeState::Completed if !node.acceptance.is_empty() => {
            "Execution ended, but acceptance evidence is still missing".to_string()
        }
        NodeState::Stale => "Owner cannot confirm liveness after reconciliation".to_string(),
        NodeState::Verified => "Acceptance evidence is satisfied".to_string(),
        NodeState::Completed => "Completed with no evidence requirement".to_string(),
        NodeState::Superseded => "A replacement node owns this work".to_string(),
        NodeState::Cancelled => "Cancelled by lifecycle owner".to_string(),
        NodeState::Failed => "Failed; inspect owner output before retrying".to_string(),
    }
}

fn provenance_text(node: &WorkNode) -> String {
    let provenance = match &node.provenance {
        Provenance::Import { ordinal, .. } => ordinal
            .map(|ordinal| format!("legacy import · ordinal {ordinal}"))
            .unwrap_or_else(|| "legacy import".to_string()),
        Provenance::ToolUpdate { tool, call_id } => {
            format!("tool {tool} · call {call_id}")
        }
        Provenance::RuntimeReconcile {
            source,
            observed_at,
        } => format!("runtime {source} · {observed_at} ms UTC"),
        Provenance::UserEdit { proposal_id } => format!("user-approved diff {proposal_id}"),
    };
    let reconcile = node
        .binding
        .as_ref()
        .and_then(|binding| binding.last_observation.as_ref())
        .map(|observation| format!("{} ms UTC", observation.observed_at))
        .unwrap_or_else(|| "never".to_string());
    format!("Source: {provenance}\nLast reconcile: {reconcile}")
}

fn last_output_ref(snapshot: &WorkGraphSnapshot, node: &WorkNode) -> Option<String> {
    node.binding
        .as_ref()
        .and_then(|binding| binding.last_observation.as_ref())
        .and_then(|observation| observation.output.as_ref())
        .map(format_evidence_ref)
        .or_else(|| {
            evidence_for(snapshot, node)
                .into_iter()
                .max_by_key(|evidence| evidence.updated_at)
                .and_then(|evidence| evidence.evidence.as_ref())
                .map(format_evidence_ref)
        })
}

fn format_evidence_ref(evidence: &crate::work_graph::EvidenceRef) -> String {
    let kind = match evidence.kind() {
        EvidenceKind::ToolRun => "tool run".to_string(),
        EvidenceKind::Artifact { .. } => "artifact".to_string(),
        EvidenceKind::TestSummary => "test summary".to_string(),
        EvidenceKind::Receipt { .. } => "receipt".to_string(),
        EvidenceKind::Approval => "approval".to_string(),
        EvidenceKind::Route => "route".to_string(),
        EvidenceKind::WebCitation {
            url, retrieved_at, ..
        } => format!("web citation · {url} · retrieved {retrieved_at}"),
    };
    let bytes = evidence
        .raw_bytes()
        .map(|bytes| format!(" · {bytes} raw bytes"))
        .unwrap_or_default();
    let truncation = if evidence.truncated() {
        " · truncated"
    } else {
        ""
    };
    format!("{} · {kind}{bytes}{truncation}", evidence.reference())
}

fn section_text(out: &mut String, title: &str, body: &str) {
    let _ = writeln!(out, "{title}\n{body}\n");
}

fn section_list(out: &mut String, title: &str, items: Vec<String>) {
    if items.is_empty() {
        section_text(out, title, "None");
    } else {
        section_text(
            out,
            title,
            &items
                .into_iter()
                .map(|item| format!("- {item}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_graph::{CompatTodoBinding, OperationBinding, WorkNodeId};

    fn operation(state: NodeState, suffix: &str) -> WorkNode {
        WorkNode {
            id: WorkNodeId::derive("work-surface-test", suffix),
            kind: NodeKind::Operation,
            title: format!("operation {suffix}"),
            state,
            acceptance: Vec::new(),
            binding: Some(OperationBinding {
                external: format!("shell:{suffix}"),
                durable: false,
                last_observation: None,
            }),
            evidence: None,
            provenance: Provenance::ToolUpdate {
                tool: "test".to_string(),
                call_id: suffix.to_string(),
            },
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn heading_counts_initializing_and_active_operations_as_running() {
        let mut snapshot = WorkGraphSnapshot::new();
        snapshot.nodes = vec![
            operation(NodeState::Initializing, "initializing"),
            operation(NodeState::Active, "active"),
            operation(NodeState::Ready, "ready"),
        ];

        let rows = graph_rows(
            &snapshot,
            None,
            Vec::new(),
            None,
            SettledFileActivity::default(),
        );

        assert_eq!(
            rows.first().map(|row| row.label.as_str()),
            Some("Work · 2 active · 0 needs input · 1 ready · 0 recent")
        );
    }

    #[test]
    fn live_projection_hides_clean_transient_receipts_without_duplicate_todo_group() {
        let todo_id = WorkNodeId::derive("work-surface-test", "todo:1");
        let todo = WorkNode {
            id: todo_id.clone(),
            kind: NodeKind::PlanStep,
            title: "Keep the durable checklist visible".to_string(),
            state: NodeState::Ready,
            acceptance: Vec::new(),
            binding: None,
            evidence: None,
            provenance: Provenance::ToolUpdate {
                tool: "work_update".to_string(),
                call_id: "todo-1".to_string(),
            },
            created_at: 1,
            updated_at: 1,
        };
        let mut snapshot = WorkGraphSnapshot::new();
        snapshot.nodes = vec![
            operation(NodeState::Completed, "settled"),
            operation(NodeState::Active, "running"),
            todo,
        ];
        snapshot.compat.todos.push(CompatTodoBinding {
            legacy_id: 1,
            node: todo_id,
            plan_index: None,
        });

        let rows = graph_rows(
            &snapshot,
            None,
            Vec::new(),
            None,
            SettledFileActivity::default(),
        );
        let labels = rows
            .iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"operation running"), "{labels:?}");
        assert!(!labels.contains(&"operation settled"), "{labels:?}");
        assert_eq!(
            labels
                .iter()
                .filter(|label| **label == "Keep the durable checklist visible")
                .count(),
            1,
            "one plan node must produce one Work row: {labels:?}"
        );
        assert!(
            !labels.iter().any(|label| label.starts_with("To-do")),
            "the ordered Work projection must not add a duplicate To-do heading: {labels:?}"
        );
        assert!(
            labels.contains(&"Keep the durable checklist visible"),
            "{labels:?}"
        );
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.title == "operation settled"),
            "projection filtering must retain the historical graph receipt"
        );
    }

    #[test]
    fn projection_keeps_durable_and_attention_terminal_operations() {
        let mut durable = operation(NodeState::Completed, "durable");
        durable.binding.as_mut().expect("binding").durable = true;
        let failed = operation(NodeState::Failed, "failed");
        let mut evidence_pending = operation(NodeState::Completed, "evidence-pending");
        evidence_pending.acceptance = vec![AcceptanceRequirement::EvidenceOfKind {
            kind: EvidenceKindTag::ToolRun,
        }];
        let mut snapshot = WorkGraphSnapshot::new();
        snapshot.nodes = vec![durable, failed, evidence_pending];

        let rows = graph_rows(
            &snapshot,
            None,
            Vec::new(),
            None,
            SettledFileActivity::default(),
        );
        let labels = rows
            .iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>();

        for expected in [
            "operation durable",
            "operation failed",
            "operation evidence-pending",
        ] {
            assert!(labels.contains(&expected), "missing {expected}: {labels:?}");
        }
    }

    #[test]
    fn projection_orders_attention_before_ready_and_recent() {
        let mut recent = operation(NodeState::Completed, "recent");
        recent.binding.as_mut().expect("binding").durable = true;
        let mut snapshot = WorkGraphSnapshot::new();
        snapshot.nodes = vec![
            recent,
            operation(NodeState::Ready, "ready"),
            operation(NodeState::Blocked, "blocked"),
            operation(NodeState::Active, "active"),
        ];

        let labels = graph_rows(
            &snapshot,
            None,
            Vec::new(),
            None,
            SettledFileActivity::default(),
        )
        .into_iter()
        .map(|row| row.label)
        .collect::<Vec<_>>();

        assert_eq!(
            labels,
            [
                "Work · 1 active · 1 needs input · 1 ready · 1 recent",
                "operation active",
                "operation blocked",
                "operation ready",
                "operation recent",
            ]
        );
    }

    #[test]
    fn activity_targets_keep_workspace_relative_paths_and_hide_external_paths() {
        let workspace = Path::new("/workspace/project");
        assert_eq!(
            privacy_safe_path(workspace, "/workspace/project/src/lib.rs").as_deref(),
            Some("src/lib.rs")
        );
        assert_eq!(
            privacy_safe_path(workspace, "/Users/alice/private.txt"),
            None
        );
        assert_eq!(privacy_safe_path(workspace, "../private.txt"), None);
        assert_eq!(safe_pattern("needle\nsecret"), "needle secret");
    }
}
