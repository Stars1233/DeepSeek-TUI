# Train 4 Result - Goal Mode

Branch: `codex/v0.8.61-train-4`

Commits:
- `875e5fd3a` `feat(goal): persist goal progress accounting`
- `e6784e538` `feat(goal): bridge visible goal accounting`
- `64409f7a9` `fix(swarm): keep prompt fanout gated`
- `9416a7a79` `feat(subagent): expose checkpoint continuation aliases`

No PR opened. No push. No tag, publish, release, version bump, Cargo.lock edit, or release build.

Note: `gh issue view 3215` could not refresh live issue state in this sandbox because network access to `api.github.com` failed. Work proceeded from the checked-in triage packet and prompt constraints.

## #3215 - Cross-Turn Goal Mode

Status: implemented the durable goal-progress substrate and wired the existing inline continuation hook to the shared goal-loop decision model.

Files:
- `crates/state/src/lib.rs`
- `crates/protocol/src/lib.rs`
- `crates/protocol/tests/parity_protocol.rs`
- `crates/core/src/lib.rs`
- `crates/tui/src/core/engine.rs`
- `crates/tui/src/core/engine/turn_loop.rs`
- `crates/tui/src/core/engine/tests.rs`
- `crates/tui/src/tools/goal.rs`

What changed:
- Added durable `ThreadGoalRecord.continuation_count` with SQLite migration.
- Added `record_thread_goal_continuation`.
- Added protocol-visible `ThreadGoal.continuation_count`.
- Added `ThreadRequest::GoalRecordProgress(ThreadGoalProgressParams)` so Train 3/resume callers can atomically accrue tokens, elapsed seconds, and continuation count into the durable goal row.
- Runtime `GoalState` now tracks `tokens_used`, `time_used_seconds`, and `continuation_count`.
- The TUI inline goal continuation hook now consults `goal_loop::decide_continuation` from visible progress and run-level budget, not only the local per-turn counter.

Train-3 seam:
- The TUI engine still owns only the runtime mutex on this path. It records per-turn usage into the runtime snapshot and exposes the durable core/protocol API, but the deep worker re-dispatcher still needs to call `goal_record_progress`/`record_thread_goal_usage` when Train 3 lands.

Tests:
- `cargo fmt` - passed
- `cargo test -p codewhale-state record_thread_goal --locked` - passed, 3 tests
- `cargo test -p codewhale-protocol thread_goal --locked` - passed, 3 tests
- `cargo test -p codewhale-core thread_goal_progress --locked` - passed, 1 test
- `cargo test -p codewhale-tui --locked goal` - passed, 35 tests

Risks:
- Full autonomous cross-turn re-dispatch still depends on the Train 3 durable worker dispatcher. This slice provides the durable store/protocol surface and decision wiring without unblocking prompt-only fanout.

## #891 + #1976 - Goal Model Bridge

Status: implemented the accounting bridge shape across durable `ThreadGoal`, runtime `GoalState`, visible `GoalSnapshot`, and TUI `HuntState`.

Files:
- `crates/tui/src/tools/goal.rs`
- `crates/tui/src/tui/app.rs`
- `crates/tui/src/tui/ui.rs`
- `crates/tui/src/tui/ui/tests.rs`
- `crates/tui/src/commands/groups/project/goal.rs`
- `crates/tui/src/commands/user_commands.rs`

What changed:
- Added `GoalSnapshot::from_thread_goal`.
- Added durable/status conversion helpers.
- Projected token/time/continuation accounting into `HuntState`.
- `/goal` display now shows durable-style token/time accounting and continuation count.
- Goal resets now clear stale accounting fields.

Tests:
- `cargo test -p codewhale-tui --locked goal` - passed, 35 tests
- `cargo test -p codewhale-tui --locked review_regression_dispatch_without_frontmatter_resets_previous_command_state` - passed, 1 test

Risks:
- A future resume/sync caller still needs to decide when to hydrate runtime `GoalState` from the durable `ThreadGoal` snapshot. The conversion and UI projection are ready.

## #2058 - Verifier-as-Judge Completion Gate

Status: implemented a verifier receipt gate before `update_goal complete`.

Files:
- `crates/tui/src/tools/goal.rs`
- `crates/tui/src/core/engine/tests.rs`

What changed:
- `update_goal` now requires `verification: { "status": "passed", "check": "...", "summary": "..." }` when `status` is `complete`.
- Missing, non-passed, or empty verification rejects completion and leaves the goal active.
- The continuation prompt now tells the worker to run or cite a concrete verifier before marking a goal complete.

Tests:
- `cargo test -p codewhale-tui --locked goal` - passed, 35 tests

Risks:
- The gate validates the verifier receipt supplied to `update_goal`; it does not itself run an external verifier tool.

## #3218 - `/swarm` Gate

Status: kept `/swarm` gated until the durable Train 3 substrate lands.

Files:
- `crates/tui/src/commands/groups/core/mod.rs`
- `docs/FLEET.md`
- `docs/MODES.md`

What changed:
- `/swarm` no longer dispatches prompt-only `agent_open` fanout.
- The command returns a clear gated error that points users to `/goal` or a bounded `/agent`.
- Fleet/mode docs now state that high-fanout swarm remains gated in v0.8.61.

Tests:
- `cargo test -p codewhale-tui --locked swarm_is_gated_until_durable_worker_substrate_lands` - passed, 1 test

Risks:
- None beyond the intentional feature gate. Unlocking should happen only after Train 3 provides durable workers and re-dispatch.

## #2029 - Sub-Agent Checkpoint/Continue Across Turns

Status: implemented the missing projection aliases for checkpointed continuation.

Files:
- `crates/tui/src/tools/subagent/mod.rs`
- `crates/tui/src/tools/subagent/tests.rs`

What changed:
- `SubAgentSessionProjection` now exposes `needs_continuation`.
- Timed-out interrupted workers with continuable checkpoints now expose `timed_out_with_checkpoint`.
- Transcript payloads include the same fields so UI/resume consumers can key off stable names.

Tests:
- `cargo test -p codewhale-tui --locked interrupted_projection_exposes_checkpoint_metadata_and_messages` - passed, 1 test
- `cargo test -p codewhale-tui --locked api_timeout_preserves_checkpoint_and_agent_eval_continues_from_it` - passed, 1 test

Risks:
- Broader `[subagents] max_runtime` / `max_turns` policy work remains outside this Train 4 slice.

## Final Status

`git status --short --branch` was clean before writing this report. The report file is the only post-code artifact.
