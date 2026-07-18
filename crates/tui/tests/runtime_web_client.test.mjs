import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import {
  applyRuntimeEvent,
  applySnapshot,
  createThreadState,
  eventStreamUrl,
  restoreDraft,
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
