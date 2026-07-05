# Workflow Authoring

Workflow has one runtime boundary: authored source lowers to typed
Rust `WorkflowSpec`, Rust validates the IR, and the scheduler/headless worker
runtime executes leaves. Authoring languages do not get hidden authority to own
files, shell, network, providers, cancellation, or TUI state.

This document keeps its legacy `WHALEFLOW_AUTHORING.md` path for compatibility.
Public UI and docs should say Workflow; internal crate and package names may
still use `whaleflow` where renaming would create churn.

## Language Choice

| Surface | Strength | Tradeoff | v0.8.60 stance |
|---|---|---|---|
| YAML / JSON IR | Simple, reviewable, no runtime | Verbose for generated workflows | Keep as interchange/debug format |
| JavaScript | Familiar object syntax and easy agent generation | Unsafe if executed as a general runtime | First-class authoring through declarative compile-only subset |
| TypeScript | Best editor/types story for workflow SDK | Needs stripping/typechecking if full TS is supported | Same compile-only subset for now; richer SDK later |

The default high-capability path is TypeScript/JavaScript authoring, but only as
a compile step. The compiler accepts a JSON-compatible object inside
`workflow({...})` from `.workflow.js` or `.workflow.ts`, lowers it to
`WorkflowSpec`, and runs the Rust validation gate. (Starlark authoring was a
bootstrap reference and has been removed; Workflow authoring is JS-only.)

## Contract

Accepted source shape:

```js
export default workflow({
  "id": "issue-audit-js",
  "goal": "Audit an issue fix with parallel agents",
  "nodes": [
    {
      "branch": {
        "id": "parallel-audit",
        "children": [
          { "agent": { "id": "code-audit", "prompt": "Review code", "agent_type": "review" } },
          { "agent": { "id": "test-audit", "prompt": "Review tests", "agent_type": "verifier" } }
        ]
      }
    },
    { "reduce": { "id": "summary", "inputs": ["code-audit", "test-audit"], "prompt": "Summarize" } }
  ]
});
```

Supported node wrappers: `agent`, `branch`, `sequence`, `reduce`,
`teacher_review`, `loop_until`, `cond`, and `expand`. Raw `WorkflowNode` JSON IR
with `kind` / `spec` also remains valid.

An `agent` node may declare `"profile": "reviewer"` to run as a named Fleet
roster profile. The name is trimmed and lowercased at compile time and must be
a single token (no whitespace, quotes, or `=`); the saved roster is resolved at
dispatch time, and explicit fields on the agent override profile defaults.

The compiler rejects effectful constructs such as `import`, `require`, `fetch`,
`process`, `Deno`, `Bun`, `child_process`, file reads/writes, `eval`, `async`,
and `await`. This is intentionally stricter than JavaScript: workflow source is
a familiar declaration format, not a second execution runtime.

## Verification

- `cargo test -p codewhale-whaleflow --locked javascript`

Current example: `workflows/issue_audit.workflow.js`.

## Agent-Written Fleet Workflows

The primary product flow is not "ask the user to write a script." The main
agent should decide when a task deserves workflow orchestration, draft the
Workflow source, show the plan for the current permission mode, and then let
the runtime compile and monitor it.

Workflow owns the plan: phases, branches, loops, reducers, and intermediate
results. Fleet owns the durable sub-agent configuration: slots, profiles,
models, tool posture, launch concurrency, leases, heartbeats, logs, receipts,
and resume/stop/restart controls. In other words, a workflow can choose and
monitor Fleet slots, but it must not become a second executor with its own shell
or filesystem authority.

Fleet launch validation applies a conservative default shape before any
Workflow IR is lowered to workers:

- up to 100 total worker agents per workflow run;
- up to 5 recursive Fleet rings;
- loops require `max_iterations`;
- dynamic `expand` nodes require `max_children` and a template.

Those limits bound the workflow population, not instantaneous launch
concurrency. A valid 100-agent workflow can still drain through a smaller Fleet
worker pool. Model selection stays per slot: a DeepSeek preset can suggest
`deepseek-v4-pro` for the orchestrator and `deepseek-v4-flash` for nearby
workers, but users and agents may override any slot when the task calls for it.
