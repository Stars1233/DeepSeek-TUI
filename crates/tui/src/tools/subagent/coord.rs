//! Narrow model-facing agent coordination tools.
//!
//! Keeps `agent` as the creation surface. These five tools wrap existing
//! SubAgentManager / mailbox / checkpoint machinery without restoring the
//! retired lifecycle theater (`agent_open` / `agent_eval` / …).

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    COMPLETED_AGENT_RETENTION, SharedSubAgentManager, SubAgentRuntime, SubAgentStatus,
    parse_agent_ref, subagent_session_projection, subagent_status_name,
    wait_for_subagents_from_input,
};
use crate::tools::registry::ToolRegistryBuilder;
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};

const COORD_WAIT_DEFAULT_TIMEOUT_SECS: u64 = 300;
const COORD_WAIT_MIN_TIMEOUT_SECS: u64 = 1;
const COORD_WAIT_MAX_TIMEOUT_SECS: u64 = 1800;
const COORD_WAIT_CHECK_INTERVAL: Duration = Duration::from_millis(250);
const RECENT_PROGRESS_LIMIT: usize = 8;
pub(super) const COORDINATION_RECORD_LIMIT: usize = 128;
const COORDINATION_INSPECT_LIMIT: usize = 24;

// ── agents/list ──────────────────────────────────────────────────────────

pub struct AgentsListTool {
    manager: SharedSubAgentManager,
}

impl AgentsListTool {
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolSpec for AgentsListTool {
    fn name(&self) -> &'static str {
        "agents/list"
    }

    fn description(&self) -> &'static str {
        "List child agents: ids, parent hierarchy, state, bounded recent progress, and token budget. Read-only coordination view — does not spawn or wake workers."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "include_archived": {
                    "type": "boolean",
                    "description": "Include prior-session / archived agents. Default false."
                },
                "agent_id": {
                    "type": "string",
                    "description": "Optional single agent id or session name to inspect."
                }
            },
            "required": []
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn is_read_only_for(&self, _input: &Value) -> bool {
        true
    }

    fn supports_parallel_for(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let include_archived = input
            .get("include_archived")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let agent_ref = parse_agent_ref(&input);

        let mut manager = self.manager.write().await;
        manager.cleanup(COMPLETED_AGENT_RETENTION);
        let summaries = if let Some(agent_ref) = agent_ref {
            let summary = manager
                .coordination_summary_for(&agent_ref, RECENT_PROGRESS_LIMIT)
                .map_err(|err| ToolError::invalid_input(err.to_string()))?;
            vec![summary]
        } else {
            manager.list_coordination_summaries(include_archived, RECENT_PROGRESS_LIMIT)
        };
        drop(manager);

        let payload = json!({
            "action": "list",
            "count": summaries.len(),
            "agents": summaries,
        });
        let mut tool_result = ToolResult::json(&payload)
            .map_err(|err| ToolError::execution_failed(err.to_string()))?;
        tool_result.metadata = Some(json!({
            "action": "list",
            "count": summaries.len(),
        }));
        Ok(tool_result)
    }
}

// ── agents/message ───────────────────────────────────────────────────────

pub struct AgentsMessageTool {
    manager: SharedSubAgentManager,
}

impl AgentsMessageTool {
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolSpec for AgentsMessageTool {
    fn name(&self) -> &'static str {
        "agents/message"
    }

    fn description(&self) -> &'static str {
        "Queue a parent message onto a child agent without waking it. The child receives the message on the next followup or natural resume. Use agents/followup when you also need to resume an idle or interrupted child."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Target child agent id or session name."
                },
                "message": {
                    "type": "string",
                    "description": "Message text to queue."
                }
            },
            "required": ["agent_id", "message"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let agent_ref =
            parse_agent_ref(&input).ok_or_else(|| ToolError::missing_field("agent_id"))?;
        let message = input
            .get("message")
            .or_else(|| input.get("text"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::missing_field("message"))?
            .to_string();

        let receipt = {
            let mut manager = self.manager.write().await;
            manager
                .queue_parent_message(&agent_ref, message, false)
                .map_err(|err| ToolError::invalid_input(err.to_string()))?
        };

        let payload = json!({
            "action": "message",
            "agent_id": receipt.agent_id,
            "queued": true,
            "woke": false,
            "queue_depth": receipt.queue_depth,
            "status": receipt.status,
            "note": "Message queued without waking the child.",
        });
        let mut tool_result = ToolResult::json(&payload)
            .map_err(|err| ToolError::execution_failed(err.to_string()))?;
        tool_result.metadata = Some(json!({
            "action": "message",
            "agent_id": receipt.agent_id,
            "woke": false,
            "queue_depth": receipt.queue_depth,
        }));
        Ok(tool_result)
    }
}

// ── agents/followup ──────────────────────────────────────────────────────

pub struct AgentsFollowupTool {
    manager: SharedSubAgentManager,
}

impl AgentsFollowupTool {
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolSpec for AgentsFollowupTool {
    fn name(&self) -> &'static str {
        "agents/followup"
    }

    fn description(&self) -> &'static str {
        "Queue a message and attempt to resume an idle or interrupted child. Running children receive the message on their next step; interrupted_continuable children keep a checkpoint and return the continuation_handle — live in-place resume is not automated yet (re-dispatch via agent)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Target child agent id or session name."
                },
                "message": {
                    "type": "string",
                    "description": "Follow-up message text."
                }
            },
            "required": ["agent_id", "message"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let agent_ref =
            parse_agent_ref(&input).ok_or_else(|| ToolError::missing_field("agent_id"))?;
        let message = input
            .get("message")
            .or_else(|| input.get("text"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::missing_field("message"))?
            .to_string();

        let receipt = {
            let mut manager = self.manager.write().await;
            manager
                .followup_child(&agent_ref, message)
                .map_err(|err| ToolError::invalid_input(err.to_string()))?
        };

        let payload = json!({
            "action": "followup",
            "agent_id": receipt.agent_id,
            "queued": true,
            "woke": receipt.woke,
            "queue_depth": receipt.queue_depth,
            "status": receipt.status,
            "continued_from_checkpoint": receipt.continued_from_checkpoint,
            "continuation_handle": receipt.continuation_handle,
            "note": receipt.note,
        });
        let mut tool_result = ToolResult::json(&payload)
            .map_err(|err| ToolError::execution_failed(err.to_string()))?;
        tool_result.metadata = Some(json!({
            "action": "followup",
            "agent_id": receipt.agent_id,
            "woke": receipt.woke,
            "continued_from_checkpoint": receipt.continued_from_checkpoint,
            "continuation_handle": receipt.continuation_handle,
        }));
        Ok(tool_result)
    }
}

// ── agents/interrupt ─────────────────────────────────────────────────────

pub struct AgentsInterruptTool {
    manager: SharedSubAgentManager,
    /// Optional caller identity for fail-closed self-interrupt checks.
    caller_agent_id: Option<String>,
}

impl AgentsInterruptTool {
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self {
            manager,
            caller_agent_id: None,
        }
    }

    #[must_use]
    #[allow(dead_code)] // arms self-interrupt fail-closed when child registries thread caller (P1.2)
    pub fn with_caller(mut self, caller_agent_id: impl Into<String>) -> Self {
        self.caller_agent_id = Some(caller_agent_id.into());
        self
    }
}

#[async_trait]
impl ToolSpec for AgentsInterruptTool {
    fn name(&self) -> &'static str {
        "agents/interrupt"
    }

    fn description(&self) -> &'static str {
        "Interrupt a running child agent, preserve its checkpoint, and return the prior state. Fails closed on root or self targets. Prefer this over cancel when you may resume later."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Child agent id or session name to interrupt."
                },
                "reason": {
                    "type": "string",
                    "description": "Optional interrupt reason recorded on the checkpoint."
                }
            },
            "required": ["agent_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let agent_ref =
            parse_agent_ref(&input).ok_or_else(|| ToolError::missing_field("agent_id"))?;
        let reason = input
            .get("reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("interrupted by parent via agents/interrupt")
            .to_string();

        let (prior, snapshot) = {
            let mut manager = self.manager.write().await;
            manager
                .interrupt_child(&agent_ref, self.caller_agent_id.as_deref(), reason)
                .map_err(|err| ToolError::invalid_input(err.to_string()))?
        };

        let worker_record = {
            let manager = self.manager.read().await;
            manager.get_worker_record(&snapshot.agent_id)
        };
        let projection = subagent_session_projection(snapshot, false, context, worker_record).await;
        let payload = json!({
            "action": "interrupt",
            "agent_id": projection.agent_id,
            "prior_status": subagent_status_name(&prior.status),
            "prior_steps_taken": prior.steps_taken,
            "status": projection.status,
            "checkpoint_preserved": projection.checkpoint.is_some(),
            "continuable": projection.continuable,
            "projection": projection,
        });
        let mut tool_result = ToolResult::json(&payload)
            .map_err(|err| ToolError::execution_failed(err.to_string()))?;
        tool_result.metadata = Some(json!({
            "action": "interrupt",
            "agent_id": payload["agent_id"],
            "checkpoint_preserved": payload["checkpoint_preserved"],
        }));
        Ok(tool_result)
    }
}

// ── agents/wait ──────────────────────────────────────────────────────────

pub struct AgentsWaitTool {
    manager: SharedSubAgentManager,
}

impl AgentsWaitTool {
    #[must_use]
    pub fn new(manager: SharedSubAgentManager) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ToolSpec for AgentsWaitTool {
    fn name(&self) -> &'static str {
        "agents/wait"
    }

    fn description(&self) -> &'static str {
        "Block until a child shows activity, settles (completion/failure/interrupt), or the timeout elapses. Prefer one wait over polling agents/list. until=completion (default) waits for settle; until=activity returns on progress or settle."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Optional specific child. When omitted, waits for the next watched child event."
                },
                "timeout_secs": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 1800,
                    "description": "Maximum seconds to block. Default 300."
                },
                "until": {
                    "type": "string",
                    "enum": ["completion", "activity"],
                    "description": "completion (default): return when a child leaves running. activity: also return when recent progress changes."
                }
            },
            "required": []
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn is_read_only_for(&self, _input: &Value) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let until = input
            .get("until")
            .and_then(Value::as_str)
            .unwrap_or("completion")
            .trim()
            .to_ascii_lowercase();

        if until == "completion" || until.is_empty() {
            let mut wait_input = input.clone();
            if wait_input.get("action").is_none() {
                wait_input["action"] = json!("wait");
            }
            return wait_for_subagents_from_input(&wait_input, Arc::clone(&self.manager), context)
                .await;
        }

        if until != "activity" {
            return Err(ToolError::invalid_input(format!(
                "Invalid until '{until}'. Use completion or activity."
            )));
        }

        wait_for_activity(&input, Arc::clone(&self.manager), context).await
    }
}

async fn wait_for_activity(
    input: &Value,
    manager: SharedSubAgentManager,
    context: &ToolContext,
) -> Result<ToolResult, ToolError> {
    let timeout_secs = input
        .get("timeout_secs")
        .or_else(|| input.get("timeout"))
        .and_then(Value::as_u64)
        .unwrap_or(COORD_WAIT_DEFAULT_TIMEOUT_SECS)
        .clamp(COORD_WAIT_MIN_TIMEOUT_SECS, COORD_WAIT_MAX_TIMEOUT_SECS);
    let timeout = Duration::from_secs(timeout_secs);
    let agent_ref = parse_agent_ref(input);

    let (watched, baseline): (Vec<String>, Vec<(String, u64)>) = {
        let manager = manager.read().await;
        if let Some(agent_ref) = &agent_ref {
            let snap = manager
                .get_result_by_ref(agent_ref)
                .map_err(|err| ToolError::invalid_input(err.to_string()))?;
            let fp = manager.activity_fingerprint(&snap.agent_id).unwrap_or(0);
            if snap.status != SubAgentStatus::Running {
                let payload = json!({
                    "action": "wait",
                    "until": "activity",
                    "reason": "already_settled",
                    "timed_out": false,
                    "agent_id": snap.agent_id,
                    "status": subagent_status_name(&snap.status),
                });
                let mut tool_result = ToolResult::json(&payload)
                    .map_err(|err| ToolError::execution_failed(err.to_string()))?;
                tool_result.metadata = Some(json!({ "action": "wait", "timed_out": false }));
                return Ok(tool_result);
            }
            (vec![snap.agent_id.clone()], vec![(snap.agent_id, fp)])
        } else {
            let running = manager
                .list_filtered(false)
                .into_iter()
                .filter(|s| s.status == SubAgentStatus::Running)
                .map(|s| s.agent_id)
                .collect::<Vec<_>>();
            let baseline = running
                .iter()
                .map(|id| {
                    let fp = manager.activity_fingerprint(id).unwrap_or(0);
                    (id.clone(), fp)
                })
                .collect();
            (running, baseline)
        }
    };

    if watched.is_empty() {
        let payload = json!({
            "action": "wait",
            "until": "activity",
            "note": "No running sub-agents; nothing to wait for.",
            "timed_out": false,
        });
        let mut tool_result = ToolResult::json(&payload)
            .map_err(|err| ToolError::execution_failed(err.to_string()))?;
        tool_result.metadata = Some(json!({ "action": "wait", "timed_out": false }));
        return Ok(tool_result);
    }

    let started = Instant::now();
    let cancelled = async {
        match &context.cancel_token {
            Some(token) => token.cancelled().await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(cancelled);

    loop {
        let outcome = {
            let manager = manager.read().await;
            let mut settled = Vec::new();
            let mut activity = Vec::new();
            for (id, base_fp) in &baseline {
                if let Ok(snap) = manager.get_result_by_ref(id) {
                    if snap.status != SubAgentStatus::Running {
                        settled.push(snap);
                        continue;
                    }
                    let fp = manager.activity_fingerprint(id).unwrap_or(0);
                    if fp != *base_fp {
                        activity.push(json!({
                            "agent_id": id,
                            "status": "running",
                            "activity_fingerprint": fp,
                        }));
                    }
                }
            }
            (settled, activity, manager.running_count())
        };

        if !outcome.0.is_empty() || !outcome.1.is_empty() {
            let payload = json!({
                "action": "wait",
                "until": "activity",
                "settled": outcome.0.iter().map(|s| json!({
                    "agent_id": s.agent_id,
                    "status": subagent_status_name(&s.status),
                })).collect::<Vec<_>>(),
                "activity": outcome.1,
                "running": outcome.2,
                "elapsed_ms": started.elapsed().as_millis(),
                "timed_out": false,
            });
            let mut tool_result = ToolResult::json(&payload)
                .map_err(|err| ToolError::execution_failed(err.to_string()))?;
            tool_result.metadata = Some(json!({
                "action": "wait",
                "timed_out": false,
                "settled": outcome.0.len(),
                "activity": outcome.1.len(),
            }));
            return Ok(tool_result);
        }

        if started.elapsed() >= timeout {
            let payload = json!({
                "action": "wait",
                "until": "activity",
                "settled": [],
                "activity": [],
                "running": outcome.2,
                "elapsed_ms": started.elapsed().as_millis(),
                "timed_out": true,
                "note": "Timed out before child activity or completion.",
            });
            let mut tool_result = ToolResult::json(&payload)
                .map_err(|err| ToolError::execution_failed(err.to_string()))?;
            tool_result.metadata = Some(json!({ "action": "wait", "timed_out": true }));
            return Ok(tool_result);
        }

        tokio::select! {
            biased;
            () = &mut cancelled => {
                return Err(ToolError::cancelled(
                    "Wait interrupted by user cancellation before child activity.".to_string(),
                ));
            }
            () = tokio::time::sleep(COORD_WAIT_CHECK_INTERVAL) => {}
        }
    }
}

/// Register the narrow coordination tools alongside `agent`.
pub fn register_coordination_tools(
    builder: ToolRegistryBuilder,
    manager: SharedSubAgentManager,
    runtime: SubAgentRuntime,
) -> ToolRegistryBuilder {
    // `runtime.parent_agent_id` is the identity of the agent this registry is
    // being built FOR: `runtime_for_nested_agent_tools` stamps the child's own
    // id there before `new_with_owner` registers tools, so anything that agent
    // spawns records it as parent. Threading it as the interrupt caller makes
    // the self-interrupt fail-closed guard live in production instead of only
    // in tests (TUI-DOG-017). `None` means the root engine registry; the root
    // is separately protected by the literal-root check in `interrupt_child`.
    let interrupt = match runtime.parent_agent_id.as_deref() {
        Some(caller) => AgentsInterruptTool::new(Arc::clone(&manager)).with_caller(caller),
        None => AgentsInterruptTool::new(Arc::clone(&manager)),
    };
    let coordinate =
        AgentsCoordinateTool::new(Arc::clone(&manager), runtime.parent_agent_id.clone());
    builder
        .with_tool(Arc::new(AgentsListTool::new(Arc::clone(&manager))))
        .with_tool(Arc::new(AgentsMessageTool::new(Arc::clone(&manager))))
        .with_tool(Arc::new(AgentsFollowupTool::new(Arc::clone(&manager))))
        .with_tool(Arc::new(interrupt))
        .with_tool(Arc::new(coordinate))
        .with_tool(Arc::new(AgentsWaitTool::new(manager)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::spec::ToolContext;
    use tempfile::tempdir;

    async fn manager_with_running_child(
        workspace: &std::path::Path,
    ) -> (SharedSubAgentManager, String) {
        let manager = Arc::new(tokio::sync::RwLock::new(
            super::super::SubAgentManager::new(workspace.to_path_buf(), 4),
        ));
        let agent_id = {
            let mut guard = manager.write().await;
            guard.insert_test_running_agent("coord_child", workspace)
        };
        (manager, agent_id)
    }

    #[tokio::test]
    async fn message_queues_without_waking() {
        let tmp = tempdir().unwrap();
        let (manager, agent_id) = manager_with_running_child(tmp.path()).await;
        let tool = AgentsMessageTool::new(Arc::clone(&manager));
        let result = tool
            .execute(
                json!({ "agent_id": agent_id, "message": "hold this" }),
                &ToolContext::new(tmp.path()),
            )
            .await
            .expect("message ok");
        let body: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(body["woke"], json!(false));
        assert_eq!(body["queued"], json!(true));
        assert_eq!(body["queue_depth"], json!(1));

        let guard = manager.read().await;
        let depth = guard.queued_mail_depth(&agent_id).unwrap();
        assert_eq!(depth, 1);
        assert!(!guard.child_was_woken(&agent_id));
    }

    #[tokio::test]
    async fn interrupt_fails_closed_on_self() {
        let tmp = tempdir().unwrap();
        let (manager, agent_id) = manager_with_running_child(tmp.path()).await;
        let tool = AgentsInterruptTool::new(Arc::clone(&manager)).with_caller(agent_id.clone());
        let err = tool
            .execute(
                json!({ "agent_id": agent_id }),
                &ToolContext::new(tmp.path()),
            )
            .await
            .expect_err("self interrupt must fail");
        let msg = err.to_string().to_ascii_lowercase();
        assert!(
            msg.contains("self") || msg.contains("own"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn interrupt_fails_closed_on_missing_target() {
        let tmp = tempdir().unwrap();
        let manager = Arc::new(tokio::sync::RwLock::new(
            super::super::SubAgentManager::new(tmp.path().to_path_buf(), 2),
        ));
        let tool = AgentsInterruptTool::new(manager);
        let err = tool
            .execute(
                json!({ "agent_id": "agent_missing" }),
                &ToolContext::new(tmp.path()),
            )
            .await
            .expect_err("missing target");
        assert!(err.to_string().contains("not found") || err.to_string().contains("Agent"));
    }

    #[tokio::test]
    async fn wait_times_out_when_child_stays_running() {
        let tmp = tempdir().unwrap();
        let (manager, agent_id) = manager_with_running_child(tmp.path()).await;
        let tool = AgentsWaitTool::new(manager);
        let result = tool
            .execute(
                json!({
                    "agent_id": agent_id,
                    "timeout_secs": 1,
                    "until": "activity"
                }),
                &ToolContext::new(tmp.path()),
            )
            .await
            .expect("wait returns");
        let body: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(body["timed_out"], json!(true));
    }

    #[tokio::test]
    async fn list_resolves_target_and_reports_queue() {
        let tmp = tempdir().unwrap();
        let (manager, agent_id) = manager_with_running_child(tmp.path()).await;
        {
            let mut guard = manager.write().await;
            guard
                .queue_parent_message(&agent_id, "note".into(), false)
                .unwrap();
        }
        let tool = AgentsListTool::new(manager);
        let result = tool
            .execute(
                json!({ "agent_id": agent_id }),
                &ToolContext::new(tmp.path()),
            )
            .await
            .expect("list ok");
        let body: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(body["count"], json!(1));
        assert_eq!(body["agents"][0]["agent_id"], json!(agent_id));
        assert!(body["agents"][0]["queued_mail"].as_u64().unwrap_or(0) >= 1);
    }

    #[tokio::test]
    async fn followup_interrupted_continuable_queues_honestly_without_auto_resume() {
        let tmp = tempdir().unwrap();
        let manager = Arc::new(tokio::sync::RwLock::new(
            super::super::SubAgentManager::new(tmp.path().to_path_buf(), 4),
        ));
        let (agent_id, handle) = {
            let mut guard = manager.write().await;
            guard.insert_test_interrupted_continuable_agent(
                "paused_child",
                tmp.path(),
                vec![crate::models::Message {
                    role: "user".to_string(),
                    content: vec![crate::models::ContentBlock::Text {
                        text: "prior work".to_string(),
                        cache_control: None,
                    }],
                }],
            )
        };
        let tool = AgentsFollowupTool::new(Arc::clone(&manager));
        let result = tool
            .execute(
                json!({ "agent_id": agent_id, "message": "please continue" }),
                &ToolContext::new(tmp.path()),
            )
            .await
            .expect("followup ok");
        let body: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(body["queued"], json!(true));
        assert_eq!(body["woke"], json!(false));
        assert_eq!(body["continued_from_checkpoint"], json!(false));
        assert_eq!(body["continuation_handle"], json!(handle));
        let note = body["note"].as_str().unwrap_or_default();
        assert!(
            note.contains("not automated") && note.contains(&handle),
            "note must fail honestly with the continuation handle: {note}"
        );

        let guard = manager.read().await;
        assert_eq!(guard.queued_mail_depth(&agent_id).unwrap(), 1);
        assert!(!guard.child_was_woken(&agent_id));
    }
}

/// Coordination records for delegated Work (#4647).
///
/// Decision records, write-scope claims, and contention detection for parallel
/// agent work. Parallel work may proceed only when scopes and contracts do not
/// collide silently.
/// Status of a coordination decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Proposed,
    Accepted,
    Superseded,
}

/// A bounded coordination decision record (#4647).
///
/// Persisted with stable subject, concise constraints, one active owner,
/// applicability scope, evidence handles, and sequence/version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionRecord {
    pub decision_id: String,
    pub subject: String,
    pub status: DecisionStatus,
    pub owner: String,
    pub scope: Vec<String>,
    pub constraints: Vec<String>,
    pub evidence_handles: Vec<String>,
    pub version: u32,
    pub sequence: u64,
}

/// A write-scope claim for a write-capable child (#4647).
///
/// Declares expected repo-relative paths/trees and named contracts.
/// This is coordination metadata, not another approval system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WriteScopeClaim {
    pub owner: String,
    pub roots: Vec<String>,
    pub exact_files: Vec<String>,
    pub contracts: Vec<String>,
}

impl WriteScopeClaim {
    /// Check whether this claim overlaps with another. A claim overlaps when
    /// either normalized tree contains the other or exact files collide.
    #[must_use]
    pub fn overlaps(&self, other: &WriteScopeClaim) -> bool {
        for root_a in &self.roots {
            for root_b in &other.roots {
                if path_contains(root_a, root_b) || path_contains(root_b, root_a) {
                    return true;
                }
            }
        }
        for file_a in &self.exact_files {
            if other.exact_files.iter().any(|f| f == file_a)
                || other.roots.iter().any(|root| path_contains(root, file_a))
            {
                return true;
            }
        }
        for file_b in &other.exact_files {
            if self.roots.iter().any(|root| path_contains(root, file_b)) {
                return true;
            }
        }
        if self
            .contracts
            .iter()
            .any(|contract| other.contracts.iter().any(|other| other == contract))
        {
            return true;
        }
        false
    }

    #[must_use]
    pub fn contains_path(&self, path: &str) -> bool {
        self.exact_files.iter().any(|file| file == path)
            || self.roots.iter().any(|root| path_contains(root, path))
    }
}

fn path_contains(root: &str, candidate: &str) -> bool {
    let root = root.trim_end_matches('/');
    let candidate = candidate.trim_end_matches('/');
    root == "."
        || root == candidate
        || candidate
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedWriteClaim {
    pub claim: WriteScopeClaim,
    pub sequence: u64,
    #[serde(default)]
    pub isolated_worktree: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationReceipt {
    pub reconciliation_id: String,
    pub subject: String,
    pub owner: String,
    pub input_decisions: Vec<String>,
    pub outcome: String,
    pub evidence_handles: Vec<String>,
    pub sequence: u64,
}

/// Durable, bounded coordination state owned by `SubAgentManager`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoordinationLedger {
    #[serde(default)]
    pub sequence: u64,
    #[serde(default)]
    pub decisions: Vec<DecisionRecord>,
    #[serde(default)]
    pub write_claims: Vec<PersistedWriteClaim>,
    #[serde(default)]
    pub reconciliations: Vec<ReconciliationReceipt>,
}

impl CoordinationLedger {
    fn next_sequence(&mut self) -> u64 {
        self.sequence = self.sequence.saturating_add(1);
        self.sequence
    }

    pub fn record_decision(
        &mut self,
        mut decision: DecisionRecord,
    ) -> Result<DecisionRecord, String> {
        if decision.subject.trim().is_empty() || decision.owner.trim().is_empty() {
            return Err("decision subject and owner are required".to_string());
        }
        decision.sequence = self.next_sequence();
        decision.version = decision.version.max(1);
        if decision.decision_id.trim().is_empty() {
            decision.decision_id = format!("decision_{}", decision.sequence);
        }
        if decision.status == DecisionStatus::Accepted {
            for existing in self.decisions.iter_mut().filter(|existing| {
                existing.subject == decision.subject && existing.status == DecisionStatus::Accepted
            }) {
                existing.status = DecisionStatus::Superseded;
            }
        }
        self.decisions.push(decision.clone());
        trim_front(&mut self.decisions, COORDINATION_RECORD_LIMIT);
        Ok(decision)
    }

    pub fn update_decision_status(
        &mut self,
        decision_id: &str,
        status: DecisionStatus,
        _owner: &str,
    ) -> Result<DecisionRecord, String> {
        let Some(index) = self
            .decisions
            .iter()
            .position(|decision| decision.decision_id == decision_id)
        else {
            return Err(format!("decision '{decision_id}' not found"));
        };
        let subject = self.decisions[index].subject.clone();
        if status == DecisionStatus::Accepted {
            for (other_index, existing) in self.decisions.iter_mut().enumerate() {
                if other_index != index
                    && existing.subject == subject
                    && existing.status == DecisionStatus::Accepted
                {
                    existing.status = DecisionStatus::Superseded;
                }
            }
        }
        let sequence = self.next_sequence();
        let decision = &mut self.decisions[index];
        decision.status = status;
        decision.version = decision.version.saturating_add(1);
        decision.sequence = sequence;
        Ok(decision.clone())
    }

    pub fn register_claim<F>(
        &mut self,
        claim: WriteScopeClaim,
        isolated_worktree: bool,
        mut owner_is_active: F,
    ) -> Result<PersistedWriteClaim, String>
    where
        F: FnMut(&str) -> bool,
    {
        if claim.owner.trim().is_empty()
            || (claim.roots.is_empty()
                && claim.exact_files.is_empty()
                && claim.contracts.is_empty())
        {
            return Err(
                "write claim requires an owner and at least one root, file, or contract"
                    .to_string(),
            );
        }
        if !isolated_worktree {
            if let Some(existing) = self.write_claims.iter().find(|existing| {
                !existing.isolated_worktree
                    && existing.claim.owner != claim.owner
                    && owner_is_active(&existing.claim.owner)
                    && existing.claim.overlaps(&claim)
            }) {
                return Err(format!(
                    "write-scope contention with {} (roots: {:?}, files: {:?}, contracts: {:?}); serialize the work, narrow the claim, or use worktree isolation",
                    existing.claim.owner,
                    existing.claim.roots,
                    existing.claim.exact_files,
                    existing.claim.contracts
                ));
            }
        }
        self.write_claims
            .retain(|existing| existing.claim.owner != claim.owner);
        let record = PersistedWriteClaim {
            claim,
            sequence: self.next_sequence(),
            isolated_worktree,
        };
        self.write_claims.push(record.clone());
        trim_front(&mut self.write_claims, COORDINATION_RECORD_LIMIT);
        Ok(record)
    }

    pub fn reconcile(
        &mut self,
        subject: String,
        owner: String,
        input_decisions: Vec<String>,
        outcome: String,
        evidence_handles: Vec<String>,
    ) -> Result<ReconciliationReceipt, String> {
        if owner.trim().is_empty() || subject.trim().is_empty() || outcome.trim().is_empty() {
            return Err("reconciliation owner, subject, and outcome are required".to_string());
        }
        if input_decisions.len() < 2 {
            return Err("neutral fan-in requires at least two input decisions".to_string());
        }
        if input_decisions.iter().any(|id| {
            !self
                .decisions
                .iter()
                .any(|decision| &decision.decision_id == id)
        }) {
            return Err("reconciliation references an unknown decision".to_string());
        }
        let inputs = input_decisions
            .iter()
            .filter_map(|id| {
                self.decisions
                    .iter()
                    .find(|decision| &decision.decision_id == id)
            })
            .collect::<Vec<_>>();
        if inputs.iter().any(|decision| decision.subject != subject) {
            return Err("reconciliation inputs must share the requested subject".to_string());
        }
        if inputs.iter().any(|decision| decision.owner == owner) {
            return Err(
                "neutral fan-in owner must differ from every input decision owner".to_string(),
            );
        }
        let sequence = self.next_sequence();
        let receipt = ReconciliationReceipt {
            reconciliation_id: format!("reconcile_{sequence}"),
            subject,
            owner,
            input_decisions,
            outcome,
            evidence_handles,
            sequence,
        };
        self.reconciliations.push(receipt.clone());
        trim_front(&mut self.reconciliations, COORDINATION_RECORD_LIMIT);
        Ok(receipt)
    }
}

fn trim_front<T>(records: &mut Vec<T>, limit: usize) {
    if records.len() > limit {
        records.drain(..records.len() - limit);
    }
}

pub struct AgentsCoordinateTool {
    manager: SharedSubAgentManager,
    caller: Option<String>,
}

impl AgentsCoordinateTool {
    #[must_use]
    pub fn new(manager: SharedSubAgentManager, caller: Option<String>) -> Self {
        Self { manager, caller }
    }
}

#[async_trait]
impl ToolSpec for AgentsCoordinateTool {
    fn name(&self) -> &'static str {
        "agents/coordinate"
    }

    fn description(&self) -> &'static str {
        "Record or inspect bounded coordination state: propose/accept/supersede decisions, expand the caller's write claim before mutation, or reconcile multiple decision records into one neutral fan-in receipt."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["inspect", "propose", "accept", "supersede", "claim", "reconcile"] },
                "decision_id": { "type": "string" },
                "subject": { "type": "string" },
                "owner": { "type": "string" },
                "scope": { "type": "array", "items": { "type": "string" } },
                "constraints": { "type": "array", "items": { "type": "string" } },
                "evidence_handles": { "type": "array", "items": { "type": "string" } },
                "roots": { "type": "array", "items": { "type": "string" } },
                "exact_files": { "type": "array", "items": { "type": "string" } },
                "contracts": { "type": "array", "items": { "type": "string" } },
                "input_decisions": { "type": "array", "items": { "type": "string" } },
                "outcome": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 24 }
            },
            "required": ["action"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }
    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }
    fn is_read_only_for(&self, input: &Value) -> bool {
        input.get("action").and_then(Value::as_str) == Some("inspect")
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let action = input
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("inspect");
        let bounded_text = |key: &str| {
            input
                .get(key)
                .and_then(Value::as_str)
                .map(|value| value.chars().take(512).collect::<String>())
        };
        let owner = self
            .caller
            .clone()
            .or_else(|| bounded_text("owner"))
            .unwrap_or_else(|| "root".to_string());
        let strings = |key: &str| {
            input
                .get(key)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .take(24)
                        .filter_map(Value::as_str)
                        .map(|value| value.chars().take(512).collect::<String>())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        let mut manager = self.manager.write().await;
        let value = match action {
            "inspect" => manager.inspect_coordination(
                bounded_text("subject").as_deref(),
                input
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(COORDINATION_INSPECT_LIMIT as u64) as usize,
            ),
            "propose" => serde_json::to_value(
                manager
                    .record_coordination_decision(DecisionRecord {
                        decision_id: bounded_text("decision_id").unwrap_or_default(),
                        subject: bounded_text("subject").unwrap_or_default(),
                        status: DecisionStatus::Proposed,
                        owner,
                        scope: strings("scope"),
                        constraints: strings("constraints"),
                        evidence_handles: strings("evidence_handles"),
                        version: 1,
                        sequence: 0,
                    })
                    .map_err(ToolError::invalid_input)?,
            )
            .map_err(|e| ToolError::execution_failed(e.to_string()))?,
            "accept" | "supersede" => serde_json::to_value(
                manager
                    .update_coordination_decision(
                        &bounded_text("decision_id").unwrap_or_default(),
                        if action == "accept" {
                            DecisionStatus::Accepted
                        } else {
                            DecisionStatus::Superseded
                        },
                        &owner,
                    )
                    .map_err(ToolError::invalid_input)?,
            )
            .map_err(|e| ToolError::execution_failed(e.to_string()))?,
            "claim" => serde_json::to_value(
                manager
                    .expand_write_claim(
                        &owner,
                        strings("roots"),
                        strings("exact_files"),
                        strings("contracts"),
                    )
                    .map_err(ToolError::invalid_input)?,
            )
            .map_err(|e| ToolError::execution_failed(e.to_string()))?,
            "reconcile" => serde_json::to_value(
                manager
                    .reconcile_coordination(
                        bounded_text("subject").unwrap_or_default(),
                        owner,
                        strings("input_decisions"),
                        bounded_text("outcome").unwrap_or_default(),
                        strings("evidence_handles"),
                    )
                    .map_err(ToolError::invalid_input)?,
            )
            .map_err(|e| ToolError::execution_failed(e.to_string()))?,
            other => {
                return Err(ToolError::invalid_input(format!(
                    "unknown coordination action '{other}'"
                )));
            }
        };
        manager.persist_state_best_effort();
        ToolResult::json(&value).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

#[cfg(test)]
mod records_tests {
    use super::*;

    #[test]
    fn overlapping_roots_detected() {
        let a = WriteScopeClaim {
            owner: "agent-a".into(),
            roots: vec!["src/tui/".into()],
            exact_files: vec![],
            contracts: vec![],
        };
        let b = WriteScopeClaim {
            owner: "agent-b".into(),
            roots: vec!["src/tui/widgets/".into()],
            exact_files: vec![],
            contracts: vec![],
        };
        assert!(a.overlaps(&b));
    }

    #[test]
    fn disjoint_roots_no_overlap() {
        let a = WriteScopeClaim {
            owner: "agent-a".into(),
            roots: vec!["src/tui/".into()],
            exact_files: vec![],
            contracts: vec![],
        };
        let b = WriteScopeClaim {
            owner: "agent-b".into(),
            roots: vec!["src/core/".into()],
            exact_files: vec![],
            contracts: vec![],
        };
        assert!(!a.overlaps(&b));
    }

    #[test]
    fn exact_file_collision_detected() {
        let a = WriteScopeClaim {
            owner: "agent-a".into(),
            roots: vec![],
            exact_files: vec!["src/main.rs".into()],
            contracts: vec![],
        };
        let b = WriteScopeClaim {
            owner: "agent-b".into(),
            roots: vec![],
            exact_files: vec!["src/main.rs".into()],
            contracts: vec![],
        };
        assert!(a.overlaps(&b));
    }

    #[test]
    fn path_overlap_respects_component_boundaries_and_root_coverage() {
        let root = WriteScopeClaim {
            owner: "agent-a".into(),
            roots: vec!["src".into()],
            exact_files: vec![],
            contracts: vec![],
        };
        let sibling = WriteScopeClaim {
            owner: "agent-b".into(),
            roots: vec!["src2".into()],
            exact_files: vec![],
            contracts: vec![],
        };
        let child_file = WriteScopeClaim {
            owner: "agent-c".into(),
            roots: vec![],
            exact_files: vec!["src/lib.rs".into()],
            contracts: vec![],
        };
        assert!(!root.overlaps(&sibling));
        assert!(root.overlaps(&child_file));
    }

    #[test]
    fn active_shared_claims_contend_but_isolated_claims_do_not() {
        let mut ledger = CoordinationLedger::default();
        let first = WriteScopeClaim {
            owner: "agent-a".into(),
            roots: vec!["src".into()],
            exact_files: vec![],
            contracts: vec!["public-api".into()],
        };
        ledger.register_claim(first, false, |_| false).unwrap();
        let second = WriteScopeClaim {
            owner: "agent-b".into(),
            roots: vec!["docs".into()],
            exact_files: vec![],
            contracts: vec!["public-api".into()],
        };
        let err = ledger
            .register_claim(second.clone(), false, |owner| owner == "agent-a")
            .unwrap_err();
        assert!(
            err.contains("contention") && err.contains("agent-a"),
            "{err}"
        );
        assert!(
            ledger
                .register_claim(second, true, |owner| owner == "agent-a")
                .is_ok()
        );
    }

    #[test]
    fn accepted_decision_supersedes_prior_acceptance_and_reconciles() {
        let mut ledger = CoordinationLedger::default();
        let make = |id: &str, owner: &str, status| DecisionRecord {
            decision_id: id.into(),
            subject: "storage".into(),
            status,
            owner: owner.into(),
            scope: vec!["router".into()],
            constraints: vec![],
            evidence_handles: vec![format!("receipt:{id}")],
            version: 1,
            sequence: 0,
        };
        ledger
            .record_decision(make("a", "agent-a", DecisionStatus::Accepted))
            .unwrap();
        ledger
            .record_decision(make("b", "agent-b", DecisionStatus::Proposed))
            .unwrap();
        ledger
            .update_decision_status("b", DecisionStatus::Accepted, "root")
            .unwrap();
        assert_eq!(ledger.decisions[0].status, DecisionStatus::Superseded);
        let receipt = ledger
            .reconcile(
                "storage".into(),
                "root".into(),
                vec!["a".into(), "b".into()],
                "use bounded origin-session artifacts".into(),
                vec!["test:coord".into()],
            )
            .unwrap();
        assert_eq!(receipt.input_decisions.len(), 2);
        assert!(receipt.sequence > ledger.decisions[1].sequence);
    }
}
