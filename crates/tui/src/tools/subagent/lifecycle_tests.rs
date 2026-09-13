use super::*;
use tempfile::tempdir;

fn prior_messages() -> Vec<Message> {
    vec![Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: "retained work".into(),
            cache_control: None,
        }],
    }]
}

#[tokio::test]
async fn lifecycle_bulk_followup_preserves_mappings_and_retries_without_duplicate_workers() {
    let dir = tempdir().unwrap();
    let manager = new_shared_subagent_manager(dir.path().to_path_buf(), 12);
    let mut sources = Vec::new();
    {
        let mut guard = manager.write().await;
        for i in 0..6 {
            let (id, _) = guard.insert_test_interrupted_continuable_agent(
                &format!("parked-{i}"),
                dir.path(),
                prior_messages(),
            );
            let agent = guard.agents.get_mut(&id).unwrap();
            agent.checkpoint.as_mut().unwrap().parked_at_turn_end = true;
            agent.agent_type = FleetRole::Scout;
            agent.model = "deepseek-v4-flash".into();
            agent.allowed_tools = Some(Vec::new());
            let spec = &mut guard.worker_records.get_mut(&id).unwrap().spec;
            spec.model = "deepseek-v4-flash".into();
            spec.agent_type = FleetRole::Scout;
            spec.runtime_profile = WorkerRuntimeProfile::for_role(FleetRole::Scout);
            sources.push(id);
        }
    }
    let (client, _, _) =
        super::tests::delayed_chat_client(Duration::from_secs(30), "fixture result").await;
    let mut runtime = super::tests::stub_runtime();
    runtime.manager = Arc::clone(&manager);
    runtime.client = client;
    runtime.context = ToolContext::new(dir.path());
    let tool = coord::AgentsFollowupTool::new(Arc::clone(&manager)).with_runtime(runtime);
    let input = json!({"agent_ids": sources, "message": "Continue the assignment."});
    let first = tool
        .execute(input.clone(), &ToolContext::new(dir.path()))
        .await
        .unwrap();
    let first: Value = serde_json::from_str(&first.content).unwrap();
    assert_eq!(first["results"].as_array().unwrap().len(), 6);
    assert_eq!(first["errors"], json!([]));
    let second = tool
        .execute(input, &ToolContext::new(dir.path()))
        .await
        .unwrap();
    let second: Value = serde_json::from_str(&second.content).unwrap();
    for (a, b) in first["results"]
        .as_array()
        .unwrap()
        .iter()
        .zip(second["results"].as_array().unwrap())
    {
        assert_eq!(a["from"], b["from"]);
        assert_eq!(a["to"], b["to"]);
        assert_ne!(a["from"], a["to"]);
    }
    let mut guard = manager.write().await;
    assert_eq!(guard.agents.len(), 12);
    for id in sources {
        let target = guard.continuation_target(&id).unwrap();
        assert_ne!(target, id);
        assert_eq!(
            guard.continuation_source(&target).as_deref(),
            Some(id.as_str())
        );
        let _ = guard.cancel_agent(&target);
    }
}

#[tokio::test]
async fn lifecycle_bulk_followup_reports_unknown_and_foreign_targets_without_hiding_success() {
    let dir = tempdir().unwrap();
    let manager = new_shared_subagent_manager(dir.path().to_path_buf(), 4);
    let (owned, foreign) = {
        let mut guard = manager.write().await;
        let owned = guard.insert_test_running_agent("owned", dir.path());
        let foreign = guard.insert_test_running_agent("foreign", dir.path());
        guard.assign_test_session_owner(&foreign, "another-session");
        (owned, foreign)
    };
    let tool = coord::AgentsFollowupTool::new(Arc::clone(&manager));
    let result = tool
        .execute(
            json!({"agent_ids": [owned, "missing", foreign], "message": "Check progress"}),
            &ToolContext::new(dir.path()),
        )
        .await
        .unwrap();
    let payload: Value = serde_json::from_str(&result.content).unwrap();
    assert_eq!(payload["results"].as_array().unwrap().len(), 1);
    assert_eq!(payload["errors"].as_array().unwrap().len(), 2);
    assert!(!manager.read().await.child_was_woken(&foreign));
}

#[tokio::test]
async fn lifecycle_followup_rejects_ambiguous_and_invalid_batch_inputs_before_delivery() {
    let dir = tempdir().unwrap();
    let manager = new_shared_subagent_manager(dir.path().to_path_buf(), 2);
    let id = manager
        .write()
        .await
        .insert_test_running_agent("owned", dir.path());
    let tool = coord::AgentsFollowupTool::new(Arc::clone(&manager));
    for input in [
        json!({"agent_id": id, "agent_ids": [id], "message": "x"}),
        json!({"agent_ids": [], "message": "x"}),
        json!({"agent_ids": [id, 7], "message": "x"}),
        json!({"all_parked": "true", "message": "x"}),
        json!({"agent_id": id, "message": "  "}),
    ] {
        assert!(
            tool.execute(input, &ToolContext::new(dir.path()))
                .await
                .is_err()
        );
    }
    assert!(!manager.read().await.child_was_woken(&id));
}

#[tokio::test]
async fn lifecycle_resume_lineage_survives_persist_and_rejects_cycles_and_foreign_hops() {
    let dir = tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let state_path = base.join(".codewhale/subagents/state.json");
    let mut manager = SubAgentManager::new(base.clone(), 6).with_state_path(state_path.clone());
    let (a, _) = manager.insert_test_interrupted_continuable_agent("old", &base, prior_messages());
    let (b, _) =
        manager.insert_test_interrupted_continuable_agent("continued", &base, prior_messages());
    let (c, _) =
        manager.insert_test_interrupted_continuable_agent("latest", &base, prior_messages());
    manager.resume_targets.insert(a.clone(), b.clone());
    manager.resume_targets.insert(b.clone(), c.clone());
    let (path, payload) = manager.build_persist_payload().unwrap().unwrap();
    write_json_atomic(&base, &path, &payload).unwrap();
    let mut loaded = SubAgentManager::new(base.clone(), 6).with_state_path(state_path);
    loaded.load_state().unwrap();
    assert_eq!(loaded.continuation_target(&a).unwrap(), c);
    loaded.resume_targets.insert(c.clone(), a.clone());
    assert!(
        loaded
            .continuation_target(&a)
            .unwrap_err()
            .to_string()
            .contains("cycle")
    );
    loaded.resume_targets.remove(&c);
    loaded.assign_test_session_owner(&c, "foreign");
    assert!(
        loaded
            .continuation_target(&a)
            .unwrap_err()
            .to_string()
            .contains("outside")
    );
}

#[tokio::test]
async fn lifecycle_compact_roster_bounds_every_state_and_pages_multibyte_names() {
    let dir = tempdir().unwrap();
    let manager = new_shared_subagent_manager(dir.path().to_path_buf(), 50);
    let mut expected = HashSet::new();
    {
        let mut guard = manager.write().await;
        for i in 0..37 {
            let id = guard.insert_test_running_agent(&format!("bounded-{i}"), dir.path());
            expected.insert(id.clone());
            let agent = guard.agents.get_mut(&id).unwrap();
            agent.session_name = "🐋\"".repeat(4000);
            agent.prompt = "archive-only".repeat(10_000);
            agent.checkpoint = Some(build_subagent_checkpoint(
                &id,
                "resume",
                &prior_messages(),
                1,
                true,
            ));
            if i % 3 == 1 {
                agent.status = SubAgentStatus::Interrupted("reason".repeat(10_000));
            }
            if i % 3 == 2 {
                agent.status = SubAgentStatus::Failed("failure".repeat(10_000));
            }
            let record = guard.worker_records.get_mut(&id).unwrap();
            record.usage.total_tokens = Some(100);
            record.usage.budget_spent_tokens = Some(3700);
            record.verification.summary = "\"🐋".repeat(10_000);
            if i > 0 {
                record.parent_run_id = Some("agent_bounded-0".into());
            }
        }
    }
    let mut offset = 0;
    let mut seen = HashSet::new();
    loop {
        let result = inspect_agent_from_input(
            &json!({"action": "status", "verbose": true, "offset": offset}),
            Arc::clone(&manager),
            &ToolContext::new(dir.path()),
            false,
            None,
        )
        .await
        .unwrap();
        assert!(
            result.content.len() <= lifecycle::COMPACT_STATUS_BYTES,
            "{}",
            result.content.len()
        );
        let value: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(value["total_count"], 37);
        assert_eq!(
            value["usage"]["total_tokens"], 3700,
            "scope totals must not be counted per child"
        );
        let rows = value["agents"].as_array().unwrap();
        assert!(!rows.is_empty());
        for row in rows {
            assert!(seen.insert(row["agent_id"].as_str().unwrap().to_string()));
            for key in [
                "snapshot",
                "worker_record",
                "checkpoint",
                "transcript_handle",
            ] {
                assert!(row.get(key).is_none(), "{key}");
            }
        }
        let Some(next) = value["next_offset"].as_u64() else {
            break;
        };
        assert!(next > offset);
        offset = next;
    }
    assert_eq!(seen, expected);
}

#[tokio::test]
async fn lifecycle_addressed_status_follows_lineage_and_detail_is_bounded() {
    let dir = tempdir().unwrap();
    let manager = new_shared_subagent_manager(dir.path().to_path_buf(), 3);
    let (old, latest) = {
        let mut guard = manager.write().await;
        let (old, _) =
            guard.insert_test_interrupted_continuable_agent("old", dir.path(), prior_messages());
        let latest = guard.insert_test_running_agent("latest", dir.path());
        guard.resume_targets.insert(old.clone(), latest.clone());
        guard.agents.get_mut(&latest).unwrap().result = Some("🐋".repeat(100_000));
        (old, latest)
    };
    let context = ToolContext::new(dir.path());
    let result = inspect_agent_from_input(
        &json!({"agent_id": old}),
        Arc::clone(&manager),
        &context,
        false,
        None,
    )
    .await
    .unwrap();
    let row: Value = serde_json::from_str(&result.content).unwrap();
    assert_eq!(row["agent_id"], latest);
    assert_eq!(row["addressed_agent_id"], old);
    assert_eq!(row["resumed_from"], old);
    assert!(row.get("snapshot").is_none());
    let detail = inspect_agent_from_input(
        &json!({"agent_id": old, "detail": true}),
        manager,
        &context,
        false,
        None,
    )
    .await
    .unwrap();
    assert!(detail.content.len() <= 32 * 1024);
    let row: Value = serde_json::from_str(&detail.content).unwrap();
    assert_eq!(row["detail_bounded"], true);
    assert!(row["transcript_handle"].is_object());
}

#[tokio::test]
async fn lifecycle_named_cancel_stops_grandchildren_and_preserves_sibling() {
    let dir = tempdir().unwrap();
    let mut manager = SubAgentManager::new(dir.path().to_path_buf(), 4);
    let parent = manager.insert_test_running_agent("parent", dir.path());
    let child = manager.insert_test_running_agent("child", dir.path());
    let sibling = manager.insert_test_running_agent("sibling", dir.path());
    let record = manager.worker_records.get_mut(&child).unwrap();
    record.parent_run_id = Some(parent.clone());
    record.spec.parent_run_id = Some(parent.clone());
    manager
        .cancel_agent_for_session("workspace", &parent)
        .unwrap();
    assert_eq!(
        manager.get_result(&child).unwrap().status,
        SubAgentStatus::Cancelled
    );
    assert_eq!(
        manager.get_result(&parent).unwrap().status,
        SubAgentStatus::Cancelled
    );
    assert_eq!(
        manager.get_result(&sibling).unwrap().status,
        SubAgentStatus::Running
    );
}

#[test]
fn lifecycle_recovery_never_forks_and_byte_preview_preserves_utf8() {
    let instruction = subagent_followup_recovery("agent_parked");
    assert!(instruction.contains("action=\"followup\""));
    assert!(!instruction.contains("resume_from"));
    let preview = lifecycle::text_preview(&"🐋".repeat(10_000), 65);
    assert!(preview.len() <= 65);
    assert!(preview.ends_with("..."));
}

#[tokio::test]
async fn lifecycle_followup_rechecks_actual_successor_authority() {
    let dir = tempdir().unwrap();
    let manager = new_shared_subagent_manager(dir.path().to_path_buf(), 4);
    let (caller, old, sibling) = {
        let mut guard = manager.write().await;
        let caller = guard.insert_test_running_agent("caller", dir.path());
        let (old, _) = guard.insert_test_interrupted_continuable_agent(
            "own-child",
            dir.path(),
            prior_messages(),
        );
        let sibling = guard.insert_test_running_agent("sibling", dir.path());
        let record = guard.worker_records.get_mut(&old).unwrap();
        record.parent_run_id = Some(caller.clone());
        record.spec.parent_run_id = Some(caller.clone());
        guard.resume_targets.insert(old.clone(), sibling.clone());
        (caller, old, sibling)
    };
    let tool =
        coord::AgentsFollowupTool::new(Arc::clone(&manager)).with_optional_caller(Some(caller));
    assert!(
        tool.execute(
            json!({"agent_id": old, "message": "Try to wake sibling"}),
            &ToolContext::new(dir.path())
        )
        .await
        .is_err()
    );
    assert!(!manager.read().await.child_was_woken(&sibling));
}

#[tokio::test]
async fn lifecycle_deliverable_preview_reports_omissions_and_detail_pages_the_full_list() {
    let dir = tempdir().unwrap();
    let mut manager = SubAgentManager::new(dir.path().to_path_buf(), 2);
    let id = manager.insert_test_running_agent("outputs", dir.path());
    manager
        .worker_records
        .get_mut(&id)
        .unwrap()
        .verification
        .deliverables = (0..9)
        .map(|index| DeliverableVerdict {
            path: format!("report-{index}.md"),
            status: if index == 8 {
                "missing".into()
            } else {
                "present".into()
            },
            bytes: (index != 8).then_some(20),
        })
        .collect();
    let compact = lifecycle::compact_row(&manager, &manager.agents[&id]);
    assert_eq!(compact["verification"]["deliverables_total"], 9);
    assert_eq!(compact["verification"]["deliverables_omitted"], 5);
    assert_eq!(compact["verification"]["deliverable_counts"]["missing"], 1);
    assert_eq!(
        compact["verification"]["deliverables"][0]["status"],
        "missing"
    );
    let detail = lifecycle::bounded_detail(
        json!({"verification": manager.worker_records[&id].verification}),
        compact,
        4,
        2,
    );
    assert_eq!(
        detail["verification"]["deliverables"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        detail["verification"]["deliverables"][0]["path"],
        "report-4.md"
    );
    assert_eq!(detail["verification"]["deliverables_next_offset"], 6);
}

#[tokio::test]
async fn lifecycle_continuation_link_is_durable_before_the_child_can_run() {
    let dir = tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let path = base.join(".codewhale/subagents/state.json");
    let manager = Arc::new(RwLock::new(
        SubAgentManager::new(base.clone(), 4).with_state_path(path.clone()),
    ));
    let (client, calls, _) =
        super::tests::delayed_chat_client(Duration::from_secs(30), "fixture").await;
    let mut runtime = super::tests::stub_runtime();
    runtime.client = client;
    runtime.manager = Arc::clone(&manager);
    runtime.context = ToolContext::new(&base);
    let mut guard = manager.write().await;
    let (source, _) =
        guard.insert_test_interrupted_continuable_agent("durable-source", &base, prior_messages());
    guard.agents.get_mut(&source).unwrap().model = "deepseek-v4-flash".into();
    let successor = guard
        .resume_from_checkpoint(Arc::clone(&manager), runtime, &source, "Continue")
        .unwrap();
    // Holding the manager lock keeps run_subagent_task_inner at its first
    // await. This is the earliest published snapshot, before any child step.
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    let persisted: PersistedSubAgentState =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        persisted.resume_targets.get(&source),
        Some(&successor.agent_id)
    );
    assert!(
        persisted
            .agents
            .iter()
            .any(|agent| agent.id == successor.agent_id)
    );
    let _ = guard.cancel_agent(&successor.agent_id);
}

#[tokio::test]
async fn lifecycle_continuation_persist_failure_rolls_back_worker_and_link() {
    let dir = tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let path = base.join(".codewhale/subagents/state.json");
    let manager = Arc::new(RwLock::new(
        SubAgentManager::new(base.clone(), 4).with_state_path(path),
    ));
    let mut runtime = super::tests::stub_runtime();
    runtime.manager = Arc::clone(&manager);
    runtime.context = ToolContext::new(&base);
    let mut guard = manager.write().await;
    let (source, _) =
        guard.insert_test_interrupted_continuable_agent("failed-source", &base, prior_messages());
    guard.agents.get_mut(&source).unwrap().model = "deepseek-v4-flash".into();
    std::fs::create_dir_all(base.join(".codewhale")).unwrap();
    std::fs::write(base.join(".codewhale/subagents"), "not a directory").unwrap();
    let result = guard.resume_from_checkpoint(Arc::clone(&manager), runtime, &source, "Continue");
    assert!(result.is_err());
    assert_eq!(guard.agents.len(), 1);
    assert_eq!(guard.worker_records.len(), 1);
    assert!(guard.resume_targets.is_empty());
    assert!(matches!(
        guard.get_result(&source).unwrap().status,
        SubAgentStatus::Interrupted(_)
    ));
}
