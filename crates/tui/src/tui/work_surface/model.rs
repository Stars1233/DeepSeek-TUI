use ratatui::layout::Rect;

use crate::localization::MessageId;
use crate::tools::todo::{TodoItem, TodoStatus};
use crate::tui::app::{App, SidebarRowAction};

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
    Worker,
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
    pub stop_action: Option<SidebarRowAction>,
}

#[derive(Debug, Clone)]
pub(super) struct WorkHitbox {
    pub id: WorkRowId,
    pub row_y: u16,
    /// Render-time Open control hitbox (TUI-DOG-005).
    pub open_zone_start_col: Option<u16>,
    pub open_zone_end_col: Option<u16>,
    pub stop_zone_start_col: Option<u16>,
    pub stop_zone_end_col: Option<u16>,
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
    /// Row-local Stop arm (TUI-DOG-006).
    pub(super) stop_arm: Option<super::interaction::StopArm>,
    /// Transient marker after confirm until the worker leaves the live set.
    pub(super) stopping: Option<WorkRowId>,
    pub(super) hitboxes: Vec<WorkHitbox>,
    pub(super) cached_todos: Vec<TodoItem>,
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
            stop_arm: None,
            stopping: None,
            hitboxes: Vec::new(),
            cached_todos: Vec::new(),
            latest_rows: Vec::new(),
        }
    }

    pub(super) fn selected_index(&self, rows: &[WorkRow]) -> Option<usize> {
        self.selected
            .as_ref()
            .and_then(|selected| rows.iter().position(|row| &row.id == selected))
    }

    pub(super) fn clamp_selection(&mut self, rows: &[WorkRow]) {
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
        let selected = self.selected_index(rows).unwrap_or_default();
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
    if let Ok(todos) = app.todos.try_lock() {
        app.work_surface.cached_todos = todos.snapshot().items;
    }

    let live = super::live_projection::LiveWorkProjection::from_app(app);
    let attention_hold = live
        .rows
        .iter()
        .any(|row| row.state == super::live_projection::LiveWorkState::Waiting);
    let todos = app.work_surface.cached_todos.clone();

    let mut rows = Vec::new();
    if live.counts.active > 0 || !live.rows.is_empty() {
        rows.push(section(
            "active",
            &format!(
                "Active {} · Tasks {} · Runs {} · Workers {}",
                live.counts.active, live.counts.tasks, live.counts.runs, live.counts.workers
            ),
            live.counts.active,
        ));
        rows.extend(live.rows.iter().map(|row| live_row(row, attention_hold)));
    }
    if !todos.is_empty() {
        let completed = todos
            .iter()
            .filter(|item| item.status == TodoStatus::Completed)
            .count();
        let label = app.tr(MessageId::SidebarTodoLabel).into_owned();
        rows.push(section(
            "todo",
            &format!("{label} {completed}/{}", todos.len()),
            todos.len(),
        ));
        rows.extend(todos.into_iter().map(todo_row));
    }
    app.work_surface.latest_rows = rows.clone();
    let stoppable: Vec<_> = rows
        .iter()
        .filter(|row| row.stop_action.is_some())
        .map(|row| row.id.clone())
        .collect();
    super::interaction::clear_stale_stopping(app, &stoppable);
    if let Some(opened) = app.work_surface.opened.as_ref()
        && !rows.iter().any(|row| &row.id == opened)
    {
        app.work_surface.opened = None;
    }
    rows
}

fn section(id: &str, label: &str, count: usize) -> WorkRow {
    WorkRow {
        id: WorkRowId(format!("section:{id}")),
        mark: "▾",
        label: if label.chars().any(char::is_numeric) {
            label.to_string()
        } else {
            format!("{label} {count}")
        },
        detail: label.to_string(),
        tone: WorkTone::Heading,
        selectable: false,
        primary_action: None,
        stop_action: None,
    }
}

fn live_row(row: &super::live_projection::LiveWorkRow, attention_hold: bool) -> WorkRow {
    let namespace = if row.kind == super::live_projection::LiveWorkKind::Run {
        "jobs"
    } else {
        "task"
    };
    let source_id = match row.kind {
        super::live_projection::LiveWorkKind::Worker => row
            .identity
            .strip_prefix("worker:")
            .unwrap_or(&row.identity)
            .to_string(),
        super::live_projection::LiveWorkKind::Run => row
            .identity
            .strip_prefix("shell:")
            .unwrap_or(&row.detail)
            .to_string(),
        _ => row.detail.clone(),
    };
    let open = format!("/{namespace} show {source_id}");
    let stoppable = row.state != super::live_projection::LiveWorkState::Settled
        && matches!(
            row.kind,
            super::live_projection::LiveWorkKind::Task | super::live_projection::LiveWorkKind::Run
        );
    let (mark, tone) = match row.state {
        super::live_projection::LiveWorkState::Active if attention_hold => ("·", WorkTone::Muted),
        _ => match row.state {
            super::live_projection::LiveWorkState::Active => (
                "›",
                if row.kind == super::live_projection::LiveWorkKind::Worker {
                    WorkTone::Worker
                } else {
                    WorkTone::Live
                },
            ),
            super::live_projection::LiveWorkState::Waiting => ("◆", WorkTone::Attention),
            super::live_projection::LiveWorkState::Settled => match row.status.as_str() {
                "completed" | "success" | "done" => ("✓", WorkTone::Success),
                "failed" | "canceled" | "cancelled" | "interrupted" => ("✕", WorkTone::Attention),
                _ => ("☐", WorkTone::Muted),
            },
        },
    };
    let (primary_action, stop_action) = match row.kind {
        super::live_projection::LiveWorkKind::Worker => {
            let agent_id = source_id
                .strip_prefix("worker:")
                .unwrap_or(&source_id)
                .to_string();
            (
                Some(SidebarRowAction::OpenAgentDetail {
                    agent_id: agent_id.clone(),
                }),
                (row.state != super::live_projection::LiveWorkState::Settled)
                    .then_some(SidebarRowAction::CancelAgent { agent_id }),
            )
        }
        _ => (
            Some(SidebarRowAction::Command(open)),
            stoppable
                .then(|| SidebarRowAction::Command(format!("/{namespace} cancel {source_id}"))),
        ),
    };
    WorkRow {
        id: WorkRowId(row.identity.clone()),
        mark,
        label: row.label.clone(),
        detail: row.detail.clone(),
        tone,
        selectable: true,
        primary_action,
        stop_action,
    }
}

fn todo_row(item: TodoItem) -> WorkRow {
    let (mark, tone) = match item.status {
        TodoStatus::Completed => ("✓", WorkTone::Success),
        TodoStatus::InProgress => ("▸", WorkTone::Live),
        TodoStatus::Pending => ("☐", WorkTone::Muted),
    };
    WorkRow {
        id: WorkRowId(format!("todo:{}", item.id)),
        mark,
        label: item.content.clone(),
        detail: format!("#{}", item.id),
        tone,
        selectable: true,
        primary_action: Some(SidebarRowAction::InspectText {
            label: item.content,
            detail: format!("#{}", item.id),
        }),
        stop_action: None,
    }
}
