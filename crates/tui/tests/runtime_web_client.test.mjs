import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import {
  STREAM_EVENT_NAMES,
  applyRuntimeEvent,
  applySnapshot,
  createThreadState,
  eventStreamUrl,
  restoreDraft,
  runtimeEventContinuity,
  saveDraft,
  setSafeText,
  snapshotThenSubscribe,
} from "../src/runtime_web/app.mjs";

function snapshot(threadId = "thread-a", latestSeq = 7) {
  return {
    thread: { id: threadId, title: "Test", model: "test", mode: "agent" },
    turns: [{ id: "turn-1", status: "in_progress" }],
    items: [
      {
        id: "item-1",
        turn_id: "turn-1",
        kind: "agent_message",
        status: "in_progress",
        summary: "",
        detail: "Hello",
      },
    ],
    latest_seq: latestSeq,
  };
}

function runtimeEvent(sequence, event, payload = {}, overrides = {}) {
  return {
    schema_version: 1,
    seq: sequence,
    event,
    kind: event,
    thread_id: "thread-a",
    turn_id: "turn-1",
    item_id: null,
    payload,
    ...overrides,
  };
}

function cssDeclarations(styles, selectorPattern) {
  const match = styles.match(new RegExp(`${selectorPattern}\\s*\\{([^}]*)\\}`));
  assert.ok(match, `missing CSS rule matching ${selectorPattern}`);
  return match[1];
}

test("embedded web client uses the Blue Stage semantic palette", async () => {
  const [styles, html] = await Promise.all([
    readFile(new URL("../src/runtime_web/styles.css", import.meta.url), "utf8"),
    readFile(new URL("../src/runtime_web/index.html", import.meta.url), "utf8"),
  ]);

  for (const token of [
    "--ink-0: #03070d",
    "--ink-1: #08111c",
    "--ink-2: #0e1729",
    "--text: #f6f2e8",
    "--action: #6aaef2",
    "--human: #f6c453",
    "--live: #4fd1c5",
    "--warning: #ff7a59",
    "--danger: #ff86b2",
    "--ok: #9bd66f",
  ]) {
    assert.match(styles, new RegExp(token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
  assert.match(
    cssDeclarations(styles, "\\.primary-button,\\s*\\.send-button"),
    /background: var\(--action\)/,
  );
  assert.match(
    cssDeclarations(styles, "\\.status-pip\\.running"),
    /background: var\(--live\)/,
  );
  assert.match(
    cssDeclarations(styles, "\\.message\\.user \\.message-body"),
    /background: rgba\(246, 196, 83/,
  );
  assert.match(
    cssDeclarations(styles, "\\.attention-card"),
    /border: 1px solid rgba\(246, 196, 83/,
  );
  assert.match(
    cssDeclarations(styles, "\\.status-banner"),
    /color: var\(--warning\)/,
  );
  assert.match(
    cssDeclarations(styles, "\\.connection-dot\\.ready"),
    /background: var\(--ok\)/,
  );
  assert.match(html, /name="theme-color" content="#03070d"/);
});

test("loads a consistent snapshot before subscribing from latest_seq", async () => {
  const state = createThreadState("thread-a");
  const order = [];
  const subscribed = await snapshotThenSubscribe({
    state,
    threadId: "thread-a",
    loadSnapshot: async () => {
      order.push("snapshot");
      return snapshot("thread-a", 42);
    },
    subscribe: (threadId, sequence) => order.push(`subscribe:${threadId}:${sequence}`),
  });

  assert.equal(subscribed, true);
  assert.deepEqual(order, ["snapshot", "subscribe:thread-a:42"]);
  assert.equal(state.latestSeq, 42);
});

test("drops a stale snapshot selection without opening an event stream", async () => {
  const state = createThreadState("thread-a");
  let current = true;
  let subscribed = false;
  const result = await snapshotThenSubscribe({
    state,
    threadId: "thread-a",
    loadSnapshot: async () => {
      current = false;
      return snapshot();
    },
    subscribe: () => {
      subscribed = true;
    },
    isCurrent: () => current,
  });
  assert.equal(result, false);
  assert.equal(subscribed, false);
});

test("reconnect cursor advances monotonically and duplicate or stale-thread events are ignored", () => {
  const state = createThreadState("thread-a");
  assert.equal(applySnapshot(state, snapshot("thread-a", 7)), true);

  assert.equal(
    applyRuntimeEvent(
      state,
      runtimeEvent(8, "item.delta", { delta: " world", kind: "agent_message" }, { item_id: "item-1" }),
    ),
    true,
  );
  assert.equal(
    applyRuntimeEvent(
      state,
      runtimeEvent(8, "item.delta", { delta: " duplicate", kind: "agent_message" }, { item_id: "item-1" }),
    ),
    false,
  );
  assert.equal(
    applyRuntimeEvent(state, runtimeEvent(99, "turn.completed", {}, { thread_id: "thread-b" })),
    false,
  );
  assert.equal(state.items.get("item-1").detail, "Hello world");
  assert.equal(state.latestSeq, 8);
  assert.equal(eventStreamUrl("thread-a", state.latestSeq), "/v1/threads/thread-a/events?since_seq=8");
});

test("uses the stream predecessor cursor to detect real gaps without assuming global sequences are contiguous", () => {
  const state = createThreadState("thread-a");
  applySnapshot(state, snapshot("thread-a", 7));

  const interleaved = runtimeEvent(
    12,
    "item.delta",
    { delta: " after other threads", kind: "agent_message" },
    { item_id: "item-1", previous_seq: 7 },
  );
  assert.equal(runtimeEventContinuity(state, interleaved), "next");
  assert.equal(applyRuntimeEvent(state, interleaved), true);
  assert.equal(state.latestSeq, 12);

  const gap = runtimeEvent(
    15,
    "approval.required",
    { approval_id: "approval-missed", tool_name: "exec_shell" },
    { previous_seq: 14 },
  );
  assert.equal(runtimeEventContinuity(state, gap), "gap");
  assert.equal(applyRuntimeEvent(state, gap), false);
  assert.equal(state.latestSeq, 12);
  assert.equal(state.approvals.has("approval-missed"), false);
});

test("registers the full emitted Runtime vocabulary and advances continuity for every event", async () => {
  const runtimeSource = await readFile(
    new URL("../src/runtime_threads.rs", import.meta.url),
    "utf8",
  );
  const emittedNames = new Set(
    [...runtimeSource.matchAll(
      /"((?:thread|turn|item|approval|user_input|sandbox|agent|tool_call)\.[a-z_]+)"/g,
    )].map((match) => match[1]),
  );
  assert.deepEqual(new Set(STREAM_EVENT_NAMES), emittedNames);
  assert.equal(STREAM_EVENT_NAMES.includes("thread.created"), false);

  const state = createThreadState("thread-a");
  applySnapshot(state, snapshot("thread-a", 7));
  let previousSeq = 7;
  for (const eventName of STREAM_EVENT_NAMES) {
    const sequence = previousSeq + 2;
    const envelope = runtimeEvent(sequence, eventName, {}, { previous_seq: previousSeq });
    assert.equal(runtimeEventContinuity(state, envelope), "next", eventName);
    assert.equal(applyRuntimeEvent(state, envelope), true, eventName);
    assert.equal(state.latestSeq, sequence, eventName);
    previousSeq = sequence;
  }
});

test("gap recovery snapshot restores approval and user-input attention before resubscribing", async () => {
  const state = createThreadState("thread-a");
  applySnapshot(state, snapshot("thread-a", 7));
  const subscriptions = [];

  const recovered = await snapshotThenSubscribe({
    state,
    threadId: "thread-a",
    loadSnapshot: async () => ({
      ...snapshot("thread-a", 15),
      pending_approvals: [{
        id: "approval-recovered",
        turn_id: "turn-1",
        tool_name: "exec_command",
        description: "Run a local check",
      }],
      pending_user_inputs: [{
        id: "input-recovered",
        turn_id: "turn-1",
        request: { questions: [{ id: "choice", question: "Continue?", options: [] }] },
      }],
      pending_dynamic_tool_calls: [{
        thread_id: "thread-a",
        turn_id: "turn-1",
        call_id: "call-recovered",
        namespace: "bench",
        tool: "lookup",
        arguments: { id: "7" },
      }],
    }),
    subscribe: (threadId, sequence) => subscriptions.push([threadId, sequence]),
  });

  assert.equal(recovered, true);
  assert.equal(state.approvals.size, 1);
  assert.equal(state.approvals.has("approval-recovered"), true);
  assert.equal(state.userInputs.size, 1);
  assert.equal(state.userInputs.has("input-recovered"), true);
  assert.equal(state.dynamicToolCalls.size, 1);
  assert.equal(state.dynamicToolCalls.get("call-recovered").tool, "lookup");
  assert.deepEqual(subscriptions, [["thread-a", 15]]);

  const duplicate = runtimeEvent(
    15,
    "approval.required",
    { approval_id: "approval-recovered", tool_name: "exec_command" },
    { previous_seq: 14 },
  );
  assert.equal(applyRuntimeEvent(state, duplicate), false);
  assert.equal(state.approvals.size, 1);
});

test("assembles deltas and replaces the live item with its settled receipt", () => {
  const state = createThreadState("thread-a");
  applySnapshot(state, { ...snapshot(), items: [], latest_seq: 1 });
  applyRuntimeEvent(
    state,
    runtimeEvent(2, "item.delta", { delta: "one", kind: "agent_message" }, { item_id: "item-new" }),
  );
  applyRuntimeEvent(
    state,
    runtimeEvent(3, "item.delta", { delta: " two", kind: "agent_message" }, { item_id: "item-new" }),
  );
  assert.equal(state.items.get("item-new").detail, "one two");

  applyRuntimeEvent(
    state,
    runtimeEvent(
      4,
      "item.completed",
      {
        item: {
          id: "item-new",
          turn_id: "turn-1",
          kind: "agent_message",
          status: "completed",
          summary: "one two",
          detail: "one two",
        },
      },
      { item_id: "item-new" },
    ),
  );
  assert.equal(state.items.get("item-new").status, "completed");
  assert.deepEqual(state.itemOrder, ["item-new"]);

  applyRuntimeEvent(
    state,
    runtimeEvent(5, "item.delta", { delta: "partial", kind: "tool_call" }, { item_id: "item-stop" }),
  );
  applyRuntimeEvent(
    state,
    runtimeEvent(
      6,
      "item.interrupted",
      {
        item: {
          id: "item-stop",
          turn_id: "turn-1",
          kind: "tool_call",
          status: "interrupted",
          summary: "Interrupted",
          detail: "partial",
        },
      },
      { item_id: "item-stop" },
    ),
  );
  assert.equal(state.items.get("item-stop").status, "interrupted");
  assert.deepEqual(state.itemOrder, ["item-new", "item-stop"]);
});

test("projects agent lifecycle receipts live and settles them without a snapshot reload", () => {
  const state = createThreadState("thread-a");
  applySnapshot(state, { ...snapshot(), items: [], latest_seq: 1 });

  const agentItem = (status, summary) => ({
    id: "item-agent",
    turn_id: "turn-1",
    kind: "status",
    status,
    summary,
    detail: summary,
  });
  applyRuntimeEvent(
    state,
    runtimeEvent(2, "agent.spawned", { item: agentItem("in_progress", "Agent spawned") }),
  );
  assert.equal(state.items.get("item-agent").status, "in_progress");
  assert.deepEqual(state.itemOrder, ["item-agent"]);

  applyRuntimeEvent(
    state,
    runtimeEvent(3, "agent.progress", { item: agentItem("in_progress", "Agent checking") }),
  );
  applyRuntimeEvent(
    state,
    runtimeEvent(4, "agent.completed", { item: agentItem("completed", "Agent completed") }),
  );
  assert.equal(state.items.get("item-agent").status, "completed");
  assert.equal(state.items.get("item-agent").summary, "Agent completed");
  assert.deepEqual(state.itemOrder, ["item-agent"]);

  applyRuntimeEvent(
    state,
    runtimeEvent(5, "agent.list", {
      item: {
        id: "item-agent-list",
        turn_id: "turn-1",
        kind: "status",
        status: "completed",
        summary: "Agent list refreshed",
        detail: "Agent list refreshed",
      },
    }),
  );
  assert.equal(state.items.get("item-agent-list").status, "completed");
  assert.deepEqual(state.itemOrder, ["item-agent", "item-agent-list"]);
  assert.equal(state.latestSeq, 5);
});

test("tracks approval and user-input attention until each is resolved", () => {
  const state = createThreadState("thread-a");
  applySnapshot(state, snapshot());
  applyRuntimeEvent(
    state,
    runtimeEvent(8, "approval.required", { approval_id: "approval-1", tool_name: "exec_shell" }),
  );
  applyRuntimeEvent(
    state,
    runtimeEvent(9, "user_input.required", {
      id: "input-1",
      request: { questions: [{ id: "choice", question: "Choose?", options: [] }] },
    }),
  );
  assert.equal(state.approvals.has("approval-1"), true);
  assert.equal(state.userInputs.has("input-1"), true);

  applyRuntimeEvent(
    state,
    runtimeEvent(10, "approval.decided", { approval_id: "approval-1", decision: "allow" }),
  );
  assert.equal(state.approvals.has("approval-1"), false);
  assert.equal(state.userInputs.has("input-1"), true);

  applyRuntimeEvent(
    state,
    runtimeEvent(11, "user_input.answered", { input_id: "input-1" }),
  );
  assert.equal(state.userInputs.has("input-1"), false);
});

test("hydrates pending attention from a reload snapshot and clears cancellation events", () => {
  const state = createThreadState("thread-a");
  const detail = {
    ...snapshot(),
    pending_approvals: [{
      id: "approval-reload",
      turn_id: "turn-1",
      tool_name: "exec_command",
      description: "Run a local check",
    }],
    pending_user_inputs: [{
      id: "input-reload",
      turn_id: "turn-1",
      request: { questions: [{ id: "choice", question: "Continue?", options: [] }] },
    }],
  };

  assert.equal(applySnapshot(state, detail), true);
  assert.equal(state.approvals.get("approval-reload").tool_name, "exec_command");
  assert.equal(state.userInputs.get("input-reload").turn_id, "turn-1");

  applyRuntimeEvent(
    state,
    runtimeEvent(8, "user_input.canceled", { id: "input-reload", terminal: true }),
  );
  assert.equal(state.userInputs.has("input-reload"), false);
});

test("dynamic tool calls hydrate and disappear exactly once across terminal variants", () => {
  const state = createThreadState("thread-a");
  assert.equal(applySnapshot(state, {
    ...snapshot(),
    pending_dynamic_tool_calls: [{
      thread_id: "thread-a",
      turn_id: "turn-1",
      call_id: "call-snapshot",
      tool: "snapshot_lookup",
      arguments: { id: "snapshot" },
    }],
  }), true);
  assert.equal(state.dynamicToolCalls.get("call-snapshot").tool, "snapshot_lookup");

  assert.equal(applyRuntimeEvent(
    state,
    runtimeEvent(8, "tool_call.requested", {
      thread_id: "thread-a",
      turn_id: "turn-1",
      call_id: "call-live",
      tool: "live_lookup",
      arguments: { id: "live" },
    }),
  ), true);
  assert.equal(state.dynamicToolCalls.size, 2);

  assert.equal(applyRuntimeEvent(
    state,
    runtimeEvent(9, "tool_call.resolved", { call_id: "call-snapshot", status: "resolved" }),
  ), true);
  assert.equal(state.dynamicToolCalls.has("call-snapshot"), false);
  assert.equal(applyRuntimeEvent(
    state,
    runtimeEvent(9, "tool_call.resolved", { call_id: "call-snapshot", status: "resolved" }),
  ), false);
  assert.equal(state.dynamicToolCalls.size, 1);

  assert.equal(applyRuntimeEvent(
    state,
    runtimeEvent(10, "tool_call.canceled", { call_id: "call-live", status: "canceled" }),
  ), true);
  assert.equal(state.dynamicToolCalls.size, 0);

  assert.equal(applyRuntimeEvent(
    state,
    runtimeEvent(11, "tool_call.requested", {
      call_id: "call-timeout",
      tool: "slow_lookup",
      arguments: {},
    }),
  ), true);
  assert.equal(state.dynamicToolCalls.has("call-timeout"), true);
  assert.equal(applyRuntimeEvent(
    state,
    runtimeEvent(12, "tool_call.timeout", { call_id: "call-timeout", status: "timeout" }),
  ), true);
  assert.equal(state.dynamicToolCalls.size, 0);
});

test("preserves drafts per thread without browser storage", () => {
  const drafts = new Map();
  saveDraft(drafts, "thread-a", "draft A");
  saveDraft(drafts, "thread-b", "draft B");
  assert.equal(restoreDraft(drafts, "thread-a"), "draft A");
  assert.equal(restoreDraft(drafts, "thread-b"), "draft B");
  saveDraft(drafts, "thread-a", "");
  assert.equal(restoreDraft(drafts, "thread-a"), "");
});

test("renders hostile Runtime text only through the textContent sink", async () => {
  const hostile = `<img src=x onerror=alert(1)><script>alert(2)</script>`;
  const fakeElement = { textContent: "" };
  setSafeText(fakeElement, hostile);
  assert.equal(fakeElement.textContent, hostile);

  const source = await readFile(new URL("../src/runtime_web/app.mjs", import.meta.url), "utf8");
  assert.equal(source.includes("inner" + "HTML"), false);
  assert.equal(source.includes("insertAdjacent" + "HTML"), false);
  assert.equal(source.includes("local" + "Storage"), false);
  assert.equal(source.includes("session" + "Storage"), false);
});
