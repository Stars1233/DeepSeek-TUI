//! Ocean work-surface ownership.
//!
//! This is the replacement boundary for the transcript-top Tasks / To-do /
//! workers UI. Legacy sidebar code may feed other treatments, but Ocean state,
//! rendering, focus, scrolling, and row actions live here as one component.

mod input;
mod interaction;
mod live_projection;
mod model;
mod render;

pub use input::{handle_key, handle_mouse};
pub use interaction::tick_stop_arm;
pub use model::{WorkSurfacePlacement, WorkSurfaceState};
pub use render::{height, render, split_chat};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend};

    use crate::config::Config;
    use crate::tools::todo::TodoStatus;
    use crate::tui::app::{App, TaskPanelEntry, TaskPanelEntryKind, TuiOptions};

    fn app() -> App {
        let options = TuiOptions {
            model: "deepseek-v4-pro".to_string(),
            workspace: PathBuf::from("."),
            config_path: None,
            config_profile: None,
            allow_shell: false,
            use_alt_screen: true,
            use_mouse_capture: true,
            use_bracketed_paste: true,
            max_subagents: 4,
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
        };
        let mut app = App::new(options, &Config::default());
        app.ui_locale = crate::localization::Locale::En;
        app
    }

    fn add_task(app: &mut App, id: &str) {
        app.task_panel.push(TaskPanelEntry {
            id: id.to_string(),
            status: "running".to_string(),
            prompt_summary: format!("task {id}"),
            duration_ms: Some(1_000),
            kind: TaskPanelEntryKind::Background,
            stale: false,
            elapsed_since_output_ms: None,
            owner_agent_id: None,
            owner_agent_name: None,
        });
    }

    #[test]
    fn projection_keeps_every_todo_reachable() {
        let mut app = app();
        add_task(&mut app, "one");
        let mut todos = app.todos.try_lock().expect("todos");
        for (text, status) in [
            ("done", TodoStatus::Completed),
            ("current", TodoStatus::InProgress),
            ("next", TodoStatus::Pending),
            ("later", TodoStatus::Pending),
        ] {
            todos.add(text.to_string(), status);
        }
        drop(todos);

        let rows = super::model::project(&mut app);
        let todo_rows = rows
            .iter()
            .filter(|row| row.id.0.starts_with("todo:"))
            .count();
        assert_eq!(todo_rows, 4);
        assert!(rows.iter().any(|row| row.label == "later"));
    }

    #[test]
    fn dedicated_workflow_panel_is_not_duplicated_into_work() {
        let mut app = app();
        app.workflow_panel = Some(crate::tui::widgets::workflow_panel::WorkflowPanel::new(
            "workflow_owner",
            "single workflow owner",
            0,
        ));

        let projection = super::live_projection::LiveWorkProjection::from_app(&app);

        assert!(projection.rows.is_empty());
        assert_eq!(projection.counts.active, 0);
        assert!(super::model::project(&mut app).is_empty());
    }

    #[test]
    fn overflow_has_panel_owned_scroll_and_stable_selection() {
        let mut app = app();
        for id in ["one", "two", "three", "four"] {
            add_task(&mut app, id);
        }
        let backend = TestBackend::new(80, 5);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        assert!(app.work_surface.total_rows > app.work_surface.visible_rows);
        assert_eq!(app.work_surface.last_area.expect("area").width, 80);

        let transcript_delta = app.viewport.pending_scroll_delta;
        let outcome = super::handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 10,
                row: 2,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(outcome.consumed);
        assert_eq!(app.viewport.pending_scroll_delta, transcript_delta);
        assert!(app.work_surface.scroll_offset > 0);
    }

    #[test]
    fn keyboard_navigation_is_panel_local_when_focused() {
        let mut app = app();
        for id in ["one", "two", "three"] {
            add_task(&mut app, id);
        }
        app.work_surface.visible_rows = 2;
        assert!(
            super::handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char('w'), KeyModifiers::ALT)
            )
            .is_some()
        );
        let first = app.work_surface.selected.clone();
        let _ = super::handle_key(&mut app, KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        assert_ne!(app.work_surface.selected, first);
        assert!(app.work_surface.focused);
    }

    #[test]
    fn printable_keys_release_panel_focus_for_the_composer() {
        let mut app = app();
        add_task(&mut app, "one");
        assert!(
            super::handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char('w'), KeyModifiers::ALT)
            )
            .is_some()
        );
        assert!(app.work_surface.focused);

        let outcome = super::handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE),
        );
        assert!(outcome.is_none(), "composer must receive the printable key");
        assert!(!app.work_surface.focused);
    }

    #[test]
    fn compact_surface_preserves_task_todo_and_stop_control() {
        let mut app = app();
        add_task(&mut app, "shell_compact");
        app.todos
            .try_lock()
            .expect("todos")
            .add("keep prompt readable".to_string(), TodoStatus::InProgress);
        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("task shell_compact"), "{text}");
        assert!(text.contains("keep prompt"), "{text}");
        assert_eq!(app.work_surface.total_rows, 2);
        assert!(
            app.work_surface
                .hitboxes
                .iter()
                .any(|hitbox| hitbox.stop_zone_start_col.is_some())
        );
    }

    #[test]
    fn waiting_row_freezes_other_live_marks() {
        let mut app = app();
        add_task(&mut app, "run");
        add_task(&mut app, "ask");
        app.task_panel[1].status = "waiting".to_string();
        let backend = TestBackend::new(100, 5);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains('◆'), "waiting keeps a still attention mark");
        assert!(
            !text.contains('›'),
            "other live marks freeze under attention"
        );
    }

    #[test]
    fn progress_only_workers_render_before_snapshot_refresh() {
        let mut app = app();
        for index in 1..=3 {
            let id = format!("agent_{index}");
            app.agent_label_map
                .insert(id.clone(), format!("Agent {index}"));
            app.agent_progress.insert(id, "starting".to_string());
        }
        let backend = TestBackend::new(80, 8);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(app.work_surface.total_rows, 4, "section plus three workers");
        assert!(text.contains("Agent 1"), "{text}");
        assert!(text.contains("Agent 3"), "{text}");
    }

    #[test]
    fn completed_fleet_strip_spends_only_rows_it_can_render() {
        let mut app = app();
        for id in ["reef", "harbor", "current"] {
            add_task(&mut app, id);
        }
        for task in &mut app.task_panel {
            task.status = "completed".to_string();
        }

        let height = super::height(&mut app, 89, 50, false);
        assert_eq!(height, 5, "section + three receipts + divider");

        let backend = TestBackend::new(89, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        let rows = (0..height)
            .map(|y| {
                (0..89)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert!(rows[0].contains("Active 0"), "{rows:?}");
        assert!(rows[1].contains("task current"), "{rows:?}");
        assert!(rows[2].contains("task harbor"), "{rows:?}");
        assert!(rows[3].contains("task reef"), "{rows:?}");
        assert!(rows[4].contains('─'), "divider missing: {rows:?}");
        assert!(
            rows.iter().all(|row| !row.trim().is_empty()),
            "content-sized strip must not reserve blank rows: {rows:?}"
        );
    }

    #[test]
    fn disappearing_work_clears_owned_mouse_state() {
        let mut app = app();
        add_task(&mut app, "gone");
        let backend = TestBackend::new(80, 8);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        assert!(app.work_surface.last_area.is_some());
        app.work_surface.focused = true;
        app.task_panel.clear();

        assert_eq!(super::height(&mut app, 80, 8, false), 0);
        assert!(app.work_surface.last_area.is_none());
        assert!(app.work_surface.hitboxes.is_empty());
        assert!(!app.work_surface.focused);
    }

    #[test]
    fn compact_surface_keeps_overflow_rows_reachable() {
        let mut app = app();
        for id in ["one", "two", "three"] {
            add_task(&mut app, id);
        }
        for text in ["first", "second", "third"] {
            app.todos
                .try_lock()
                .expect("todos")
                .add(text.to_string(), TodoStatus::Pending);
        }
        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        assert_eq!(app.work_surface.total_rows, 6);
        app.work_surface.focused = true;
        let _ = super::handle_key(&mut app, KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        assert!(app.work_surface.scroll_offset > 0);
        assert!(
            app.work_surface
                .selected
                .as_ref()
                .is_some_and(|id| id.0.starts_with("todo:"))
        );
    }

    #[test]
    fn left_and_right_placements_reserve_a_side_rail() {
        for (placement, expected_chat_x, expected_rail_x) in [
            (super::WorkSurfacePlacement::Left, 30, 0),
            (super::WorkSurfacePlacement::Right, 0, 70),
        ] {
            let mut app = app();
            add_task(&mut app, "rail");
            app.work_surface.placement = placement;
            assert_eq!(super::height(&mut app, 100, 24, false), 0);

            let area = ratatui::layout::Rect::new(0, 0, 100, 12);
            let (chat, rail) = super::split_chat(&mut app, area, false);
            let rail = rail.expect("side rail");
            assert_eq!(chat.x, expected_chat_x);
            assert_eq!(chat.width, 70);
            assert_eq!(rail.x, expected_rail_x);
            assert_eq!(rail.width, 30);

            let backend = TestBackend::new(100, 12);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|frame| super::render(frame, rail, &mut app))
                .expect("draw");
            assert_eq!(app.work_surface.last_area, Some(rail));
            let divider_x = if placement == super::WorkSurfacePlacement::Left {
                rail.right().saturating_sub(1)
            } else {
                rail.x
            };
            assert_eq!(terminal.backend().buffer()[(divider_x, 0)].symbol(), "│");
        }
    }

    #[test]
    fn side_rail_mouse_capture_stays_inside_the_rail() {
        let mut app = app();
        for id in ["one", "two", "three", "four"] {
            add_task(&mut app, id);
        }
        app.work_surface.placement = super::WorkSurfacePlacement::Right;
        assert_eq!(super::height(&mut app, 100, 24, false), 0);
        let area = ratatui::layout::Rect::new(0, 0, 100, 4);
        let (chat, rail) = super::split_chat(&mut app, area, false);
        let rail = rail.expect("right rail");
        let backend = TestBackend::new(100, 4);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, rail, &mut app))
            .expect("draw");

        let inside = super::handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: rail.x.saturating_add(2),
                row: rail.y.saturating_add(1),
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(inside.consumed);
        assert!(app.work_surface.scroll_offset > 0);

        let outside = super::handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: chat.x.saturating_add(2),
                row: chat.y.saturating_add(1),
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(!outside.consumed);
    }

    #[test]
    fn classic_and_narrow_layouts_keep_the_existing_top_surface() {
        let mut app = app();
        add_task(&mut app, "top");
        app.work_surface.placement = super::WorkSurfacePlacement::Right;

        assert_eq!(super::height(&mut app, 100, 24, true), 3);
        let area = ratatui::layout::Rect::new(0, 0, 100, 12);
        let (chat, rail) = super::split_chat(&mut app, area, true);
        assert_eq!(chat, area);
        assert!(rail.is_none());
        assert_eq!(
            app.work_surface.placement,
            super::WorkSurfacePlacement::Right,
            "Classic fallback must not overwrite the saved Ocean preference"
        );

        assert_eq!(super::height(&mut app, 60, 16, false), 3);
        let narrow = ratatui::layout::Rect::new(0, 0, 60, 8);
        let (chat, rail) = super::split_chat(&mut app, narrow, false);
        assert_eq!(chat, narrow);
        assert!(rail.is_none());
    }

    #[test]
    fn enter_toggles_already_opened_worker_closed() {
        let mut app = app();
        for index in 1..=2 {
            let id = format!("agent_{index}");
            app.agent_label_map
                .insert(id.clone(), format!("Agent {index}"));
            app.agent_progress.insert(id, "running".to_string());
        }
        app.work_surface.focused = true;
        let rows = super::model::project(&mut app);
        let worker = rows
            .iter()
            .find(|row| row.id.0 == "worker:agent_1")
            .expect("worker");
        app.work_surface.selected = Some(worker.id.clone());
        let open = worker.primary_action.clone();
        assert!(super::interaction::activate_primary(&mut app, &worker.id, open.clone()).is_some());
        assert_eq!(app.work_surface.opened.as_ref(), Some(&worker.id));
        assert!(super::interaction::activate_primary(&mut app, &worker.id, open).is_none());
        assert!(app.work_surface.opened.is_none());
        assert_eq!(app.work_surface.selected.as_ref(), Some(&worker.id));
    }

    #[test]
    fn stop_first_activation_arms_row_with_visible_confirm() {
        let mut app = app();
        app.agent_label_map
            .insert("agent_1".into(), "Agent 1".into());
        app.agent_progress
            .insert("agent_1".into(), "running".into());
        let backend = TestBackend::new(100, 8);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        let worker = app
            .work_surface
            .latest_rows
            .iter()
            .find(|row| row.id.0 == "worker:agent_1")
            .expect("worker")
            .clone();
        let stop = worker.stop_action.clone().expect("stop");
        assert!(super::interaction::activate_stop(&mut app, &worker.id, stop.clone()).is_none());
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("confirm"), "{text}");
        assert!(text.contains("Esc"), "{text}");
        let confirmed =
            super::interaction::activate_stop(&mut app, &worker.id, stop).expect("fire");
        assert!(matches!(
            confirmed,
            crate::tui::app::SidebarRowAction::CancelAgent { agent_id } if agent_id == "agent_1"
        ));
    }

    #[test]
    fn todo_open_records_hitbox_and_opens_pager() {
        let mut app = app();
        app.todos
            .try_lock()
            .expect("todos")
            .add("ship underwater strip".into(), TodoStatus::InProgress);
        let backend = TestBackend::new(100, 8);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::render(frame, frame.area(), &mut app))
            .expect("draw");
        let hit = app
            .work_surface
            .hitboxes
            .iter()
            .find(|hit| hit.id.0.starts_with("todo:"))
            .expect("todo hitbox");
        assert!(hit.open_zone_start_col.is_some());
        assert!(hit.open_zone_end_col.is_some());
        let open_col = hit.open_zone_start_col.expect("open");
        let row_y = hit.row_y;
        let outcome = super::handle_mouse(
            &mut app,
            MouseEvent {
                kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: open_col,
                row: row_y,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(outcome.consumed);
        assert!(matches!(
            outcome.action,
            Some(crate::tui::app::SidebarRowAction::InspectText { .. })
        ));
        crate::tui::mouse_ui::apply_sidebar_row_action(&mut app, outcome.action.expect("action"));
        assert_eq!(
            app.view_stack.top_kind(),
            Some(crate::tui::views::ModalKind::Pager)
        );
        assert!(app.work_surface.opened.is_some());
    }

    #[test]
    fn focus_claim_clears_transcript_selection_owner() {
        use crate::tui::selection::TranscriptSelectionPoint;
        let mut app = app();
        add_task(&mut app, "one");
        app.viewport.transcript_selection.anchor = Some(TranscriptSelectionPoint {
            line_index: 0,
            column: 0,
        });
        app.viewport.transcript_selection.head = app.viewport.transcript_selection.anchor;
        assert!(
            super::handle_key(
                &mut app,
                KeyEvent::new(KeyCode::Char('w'), KeyModifiers::ALT)
            )
            .is_some()
        );
        assert!(app.work_surface.focused);
        assert!(!app.viewport.transcript_selection.is_active());
    }

    #[test]
    fn placements_share_keyboard_toggle_and_stop_arm() {
        for placement in [
            super::WorkSurfacePlacement::Top,
            super::WorkSurfacePlacement::Left,
            super::WorkSurfacePlacement::Right,
        ] {
            let mut app = app();
            app.work_surface.placement = placement;
            app.agent_label_map
                .insert("agent_1".into(), "Agent 1".into());
            app.agent_progress
                .insert("agent_1".into(), "running".into());
            let area = ratatui::layout::Rect::new(0, 0, 100, 12);
            let render_area = match placement {
                super::WorkSurfacePlacement::Top => {
                    let _ = super::height(&mut app, 100, 24, false);
                    area
                }
                super::WorkSurfacePlacement::Left | super::WorkSurfacePlacement::Right => {
                    // Project rows before split_chat so the side rail exists.
                    let _ = super::model::project(&mut app);
                    let (_, rail) = super::split_chat(&mut app, area, false);
                    rail.expect("rail")
                }
            };
            let backend = TestBackend::new(100, 12);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|frame| super::render(frame, render_area, &mut app))
                .expect("draw");
            app.work_surface.focused = true;
            let worker_id = app
                .work_surface
                .latest_rows
                .iter()
                .find(|row| row.id.0.starts_with("worker:"))
                .map(|row| row.id.clone())
                .expect("worker");
            app.work_surface.selected = Some(worker_id.clone());
            let open = Some(crate::tui::app::SidebarRowAction::OpenAgentDetail {
                agent_id: "agent_1".into(),
            });
            assert!(
                super::interaction::activate_primary(&mut app, &worker_id, open.clone()).is_some(),
                "{placement:?}"
            );
            assert!(
                super::interaction::activate_primary(&mut app, &worker_id, open).is_none(),
                "{placement:?}"
            );
            let stop = crate::tui::app::SidebarRowAction::CancelAgent {
                agent_id: "agent_1".into(),
            };
            assert!(
                super::interaction::activate_stop(&mut app, &worker_id, stop.clone()).is_none(),
                "{placement:?}"
            );
            assert!(
                super::interaction::activate_stop(&mut app, &worker_id, stop).is_some(),
                "{placement:?}"
            );
        }
    }

    #[test]
    fn moving_selection_clears_armed_stop() {
        let mut app = app();
        for index in 1..=2 {
            let id = format!("agent_{index}");
            app.agent_label_map
                .insert(id.clone(), format!("Agent {index}"));
            app.agent_progress.insert(id, "running".into());
        }
        app.work_surface.focused = true;
        let rows = super::model::project(&mut app);
        let selectable: Vec<_> = rows
            .iter()
            .filter(|row| row.selectable)
            .map(|row| row.id.clone())
            .collect();
        assert!(selectable.len() >= 2);
        let first = selectable[0].clone();
        let second = selectable[1].clone();
        app.work_surface.selected = Some(first.clone());
        let stop = crate::tui::app::SidebarRowAction::CancelAgent {
            agent_id: first.0.trim_start_matches("worker:").to_string(),
        };
        assert!(super::interaction::activate_stop(&mut app, &first, stop).is_none());
        app.work_surface.selected = Some(second);
        super::interaction::on_selection_changed(&mut app);
        assert!(app.work_surface.stop_arm.is_none());
    }
}
