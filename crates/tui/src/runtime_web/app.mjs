export const STREAM_EVENT_NAMES = [
  "thread.started",
  "thread.updated",
  "thread.forked",
  "turn.started",
  "turn.lifecycle",
  "turn.steered",
  "turn.interrupt_requested",
  "turn.completed",
  "item.started",
  "item.delta",
  "item.completed",
  "item.failed",
  "item.interrupted",
  "approval.required",
  "approval.decided",
  "approval.timeout",
  "user_input.required",
  "user_input.answered",
  "user_input.canceled",
  "sandbox.denied",
  "agent.spawned",
  "agent.progress",
  "agent.completed",
  "agent.list",
  "tool_call.requested",
  "tool_call.resolved",
  "tool_call.canceled",
  "tool_call.timeout",
];

export function createThreadState(threadId = "") {
  return {
    threadId,
    thread: null,
    turns: new Map(),
    turnOrder: [],
    items: new Map(),
    itemOrder: [],
    latestSeq: 0,
    approvals: new Map(),
    userInputs: new Map(),
    dynamicToolCalls: new Map(),
  };
}

export function applySnapshot(state, detail, expectedThreadId = state.threadId) {
  if (!detail || !detail.thread || detail.thread.id !== expectedThreadId) {
    return false;
  }
  state.threadId = expectedThreadId;
  state.thread = detail.thread;
  state.turns = new Map();
  state.turnOrder = [];
  for (const turn of Array.isArray(detail.turns) ? detail.turns : []) {
    if (!turn || !turn.id) continue;
    state.turns.set(turn.id, turn);
    state.turnOrder.push(turn.id);
  }
  state.items = new Map();
  state.itemOrder = [];
  for (const item of Array.isArray(detail.items) ? detail.items : []) {
    if (!item || !item.id) continue;
    state.items.set(item.id, item);
    state.itemOrder.push(item.id);
  }
  state.latestSeq = normalizedSequence(detail.latest_seq);
  state.approvals = new Map();
  for (const approval of Array.isArray(detail.pending_approvals) ? detail.pending_approvals : []) {
    const approvalId = approval?.approval_id || approval?.id;
    if (approvalId) state.approvals.set(approvalId, approval);
  }
  state.userInputs = new Map();
  for (const input of Array.isArray(detail.pending_user_inputs) ? detail.pending_user_inputs : []) {
    const inputId = input?.input_id || input?.id;
    if (inputId) state.userInputs.set(inputId, input);
  }
  state.dynamicToolCalls = new Map();
  for (const call of Array.isArray(detail.pending_dynamic_tool_calls) ? detail.pending_dynamic_tool_calls : []) {
    if (call?.call_id) state.dynamicToolCalls.set(call.call_id, call);
  }
  return true;
}

export function applyRuntimeEvent(state, envelope) {
  if (runtimeEventContinuity(state, envelope) !== "next") {
    return false;
  }
  const sequence = normalizedSequence(envelope.seq);
  state.latestSeq = sequence;

  const eventName = envelope.event || envelope.kind || "";
  const payload = envelope.payload && typeof envelope.payload === "object"
    ? envelope.payload
    : {};

  if (
    (eventName === "thread.started" || eventName === "thread.updated" || eventName === "thread.forked")
    && payload.thread
  ) {
    state.thread = payload.thread;
  } else if (eventName === "turn.started" || eventName === "turn.completed") {
    if (payload.turn) upsertTurn(state, payload.turn);
  } else if (eventName === "turn.lifecycle") {
    const turnId = envelope.turn_id;
    const turn = turnId ? state.turns.get(turnId) : null;
    if (turn && payload.status) {
      state.turns.set(turnId, { ...turn, status: payload.status });
    }
  } else if (eventName === "turn.interrupt_requested") {
    const turnId = envelope.turn_id;
    const turn = turnId ? state.turns.get(turnId) : null;
    if (turn) state.turns.set(turnId, { ...turn, status: "in_progress" });
  } else if (
    eventName === "item.started"
    || eventName === "item.completed"
    || eventName === "item.failed"
    || eventName === "item.interrupted"
    || eventName === "agent.spawned"
    || eventName === "agent.progress"
    || eventName === "agent.completed"
    || eventName === "agent.list"
  ) {
    if (payload.item) upsertItem(state, payload.item);
  } else if (eventName === "item.delta") {
    appendItemDelta(state, envelope.item_id, payload);
  } else if (eventName === "approval.required") {
    const approvalId = payload.approval_id || payload.id;
    if (approvalId) state.approvals.set(approvalId, payload);
  } else if (eventName === "approval.decided" || eventName === "approval.timeout") {
    const approvalId = payload.approval_id || payload.id;
    if (approvalId) state.approvals.delete(approvalId);
  } else if (eventName === "user_input.required") {
    const inputId = payload.id;
    if (inputId) state.userInputs.set(inputId, payload);
  } else if (eventName === "user_input.answered" || eventName === "user_input.canceled") {
    const inputId = payload.input_id || payload.id;
    if (inputId) state.userInputs.delete(inputId);
  } else if (eventName === "tool_call.requested") {
    if (payload.call_id) state.dynamicToolCalls.set(payload.call_id, payload);
  } else if (
    eventName === "tool_call.resolved"
    || eventName === "tool_call.canceled"
    || eventName === "tool_call.timeout"
  ) {
    if (payload.call_id) state.dynamicToolCalls.delete(payload.call_id);
  }
  return true;
}

export function runtimeEventContinuity(state, envelope) {
  if (!envelope || envelope.thread_id !== state.threadId) {
    return "ignore";
  }
  const sequence = normalizedSequence(envelope.seq);
  if (sequence <= state.latestSeq) {
    return "ignore";
  }
  if (Object.hasOwn(envelope, "previous_seq")) {
    const previousSequence = normalizedSequence(envelope.previous_seq);
    if (previousSequence !== state.latestSeq) {
      return "gap";
    }
  }
  return "next";
}

export async function snapshotThenSubscribe({
  state,
  threadId,
  loadSnapshot,
  subscribe,
  isCurrent = () => true,
}) {
  const detail = await loadSnapshot(threadId);
  if (!isCurrent() || !applySnapshot(state, detail, threadId)) {
    return false;
  }
  if (!isCurrent()) return false;
  subscribe(threadId, state.latestSeq);
  return true;
}

export function eventStreamUrl(threadId, latestSeq) {
  return `/v1/threads/${encodeURIComponent(threadId)}/events?since_seq=${normalizedSequence(latestSeq)}`;
}

export function saveDraft(drafts, threadId, value) {
  if (!threadId) return;
  if (value) drafts.set(threadId, value);
  else drafts.delete(threadId);
}

export function restoreDraft(drafts, threadId) {
  return drafts.get(threadId) || "";
}

export function setSafeText(element, value) {
  element.textContent = value == null ? "" : String(value);
  return element;
}

function normalizedSequence(value) {
  const sequence = Number(value);
  return Number.isSafeInteger(sequence) && sequence > 0 ? sequence : 0;
}

function upsertTurn(state, turn) {
  if (!turn || !turn.id) return;
  if (!state.turns.has(turn.id)) state.turnOrder.push(turn.id);
  state.turns.set(turn.id, turn);
}

function upsertItem(state, item) {
  if (!item || !item.id) return;
  if (!state.items.has(item.id)) state.itemOrder.push(item.id);
  state.items.set(item.id, item);
}

function appendItemDelta(state, itemId, payload) {
  if (!itemId) return;
  const delta = typeof payload.delta === "string" ? payload.delta : "";
  const existing = state.items.get(itemId) || {
    id: itemId,
    turn_id: "",
    kind: payload.kind || "agent_message",
    status: "in_progress",
    summary: "",
    detail: "",
  };
  if (!state.items.has(itemId)) state.itemOrder.push(itemId);
  state.items.set(itemId, {
    ...existing,
    status: "in_progress",
    detail: `${existing.detail || ""}${delta}`,
  });
}

function startBrowserClient() {
  const dom = {
    shell: document.querySelector("#app-shell"),
    railOpen: document.querySelector("#rail-open"),
    railClose: document.querySelector("#rail-close"),
    railScrim: document.querySelector("#rail-scrim"),
    search: document.querySelector("#thread-search"),
    threadList: document.querySelector("#thread-list"),
    newThread: document.querySelector("#new-thread"),
    connectionDot: document.querySelector("#connection-dot"),
    connectionLabel: document.querySelector("#connection-label"),
    kicker: document.querySelector("#session-kicker"),
    title: document.querySelector("#session-title"),
    facts: document.querySelector("#session-facts"),
    rename: document.querySelector("#rename-thread"),
    archive: document.querySelector("#archive-thread"),
    status: document.querySelector("#status-banner"),
    transcript: document.querySelector("#transcript"),
    attention: document.querySelector("#attention"),
    composer: document.querySelector("#composer"),
    composerInput: document.querySelector("#composer-input"),
    send: document.querySelector("#send-message"),
    interrupt: document.querySelector("#interrupt-turn"),
    renameDialog: document.querySelector("#rename-dialog"),
    renameForm: document.querySelector("#rename-form"),
    renameInput: document.querySelector("#rename-input"),
  };

  const app = {
    summaries: [],
    selectedThreadId: "",
    threadState: createThreadState(),
    workspace: null,
    runtimeInfo: null,
    drafts: new Map(),
    stream: null,
    reconnectTimer: null,
    generation: 0,
    searchTimer: null,
  };

  function element(tag, className, text) {
    const created = document.createElement(tag);
    if (className) created.className = className;
    if (text != null) setSafeText(created, text);
    return created;
  }

  function closeRail() {
    dom.shell.classList.remove("rail-visible");
    dom.railOpen.focus({ preventScroll: true });
  }

  function setConnection(kind, message) {
    dom.connectionDot.className = `connection-dot ${kind || ""}`.trim();
    setSafeText(dom.connectionLabel, message);
  }

  function showStatus(message) {
    setSafeText(dom.status, message || "");
    dom.status.hidden = !message;
  }

  async function api(path, options = {}) {
    const headers = new Headers(options.headers || {});
    if (options.body != null && !headers.has("content-type")) {
      headers.set("content-type", "application/json");
    }
    const response = await fetch(path, {
      ...options,
      headers,
      credentials: "same-origin",
      cache: "no-store",
    });
    if (!response.ok) {
      let message = `${response.status} ${response.statusText}`.trim();
      try {
        const body = await response.json();
        message = body?.error?.message || body?.message || message;
      } catch (_error) {
        // The status line is enough when the response is not JSON.
      }
      if (response.status === 401) {
        message = "This browser session is not authenticated. Restart `codewhale web` to open a fresh one-time session.";
      }
      throw new Error(message);
    }
    if (response.status === 204) return null;
    const contentType = response.headers.get("content-type") || "";
    return contentType.includes("application/json") ? response.json() : response.text();
  }

  function renderThreadList() {
    dom.threadList.replaceChildren();
    if (app.summaries.length === 0) {
      const empty = element("p", "thread-preview", "No matching threads");
      empty.style.padding = "8px 10px";
      dom.threadList.append(empty);
      return;
    }
    for (const summary of app.summaries) {
      const row = element("button", "thread-row");
      row.type = "button";
      row.dataset.threadId = summary.id;
      row.setAttribute("aria-current", summary.id === app.selectedThreadId ? "true" : "false");
      const titleRow = element("span", "thread-title-row");
      titleRow.append(element("span", "thread-title", summary.title || "New thread"));
      const status = element("span", `status-pip ${summary.latest_turn_status === "inprogress" || summary.latest_turn_status === "in_progress" ? "running" : summary.latest_turn_status === "failed" ? "failed" : ""}`);
      status.setAttribute("aria-label", summary.latest_turn_status || "idle");
      titleRow.append(status);
      row.append(titleRow);
      row.append(element("span", "thread-preview", summary.preview || "No messages yet"));
      const branch = summary.branch || basename(summary.workspace) || "local";
      row.append(element("span", "thread-meta", `${branch} · ${relativeTime(summary.updated_at)}`));
      row.addEventListener("click", () => selectThread(summary.id));
      dom.threadList.append(row);
    }
  }

  async function loadThreads(search = dom.search.value.trim()) {
    const query = new URLSearchParams({ limit: "100" });
    if (search) query.set("search", search);
    app.summaries = await api(`/v1/threads/summary?${query.toString()}`);
    renderThreadList();
    return app.summaries;
  }

  function stopStream() {
    if (app.stream) app.stream.close();
    app.stream = null;
    if (app.reconnectTimer) clearTimeout(app.reconnectTimer);
    app.reconnectTimer = null;
  }

  async function selectThread(threadId) {
    if (!threadId) return;
    saveDraft(app.drafts, app.selectedThreadId, dom.composerInput.value);
    stopStream();
    app.selectedThreadId = threadId;
    app.threadState = createThreadState(threadId);
    app.generation += 1;
    const generation = app.generation;
    dom.composerInput.value = restoreDraft(app.drafts, threadId);
    resizeComposer();
    renderThreadList();
    renderAll();
    closeRailIfNarrow();
    setConnection("", "Loading thread snapshot…");
    showStatus("");

    try {
      const subscribed = await snapshotThenSubscribe({
        state: app.threadState,
        threadId,
        loadSnapshot: (id) => api(`/v1/threads/${encodeURIComponent(id)}`),
        subscribe: (id, sequence) => connectStream(id, sequence, generation),
        isCurrent: () => generation === app.generation && threadId === app.selectedThreadId,
      });
      if (!subscribed) return;
      renderAll();
      setConnection("ready", "Local runtime connected");
    } catch (error) {
      if (generation !== app.generation) return;
      showStatus(error.message);
      setConnection("error", "Runtime connection failed");
    }
  }

  function connectStream(threadId, sequence, generation) {
    if (generation !== app.generation || threadId !== app.selectedThreadId) return;
    if (app.stream) app.stream.close();
    const stream = new EventSource(eventStreamUrl(threadId, sequence), { withCredentials: true });
    app.stream = stream;
    stream.onopen = () => setConnection("ready", "Local runtime connected");
    const receive = (message) => {
      if (
        app.stream !== stream
        || generation !== app.generation
        || threadId !== app.selectedThreadId
      ) return;
      try {
        const envelope = JSON.parse(message.data);
        if (runtimeEventContinuity(app.threadState, envelope) === "gap") {
          showStatus("Runtime event continuity changed; refreshing the thread snapshot…");
          void recoverProjection(threadId, generation, stream);
          return;
        }
        if (!applyRuntimeEvent(app.threadState, envelope)) return;
        renderAll(true);
        if (envelope.event === "turn.completed" || envelope.event === "thread.updated") {
          loadThreads().catch((error) => showStatus(error.message));
        }
      } catch (error) {
        showStatus(`Could not read a Runtime event: ${error.message}`);
      }
    };
    for (const name of STREAM_EVENT_NAMES) stream.addEventListener(name, receive);
    stream.onerror = () => {
      if (app.stream !== stream) {
        stream.close();
        return;
      }
      stream.close();
      app.stream = null;
      if (generation !== app.generation || threadId !== app.selectedThreadId) return;
      setConnection("", "Reconnecting to local runtime…");
      app.reconnectTimer = setTimeout(
        () => connectStream(threadId, app.threadState.latestSeq, generation),
        900,
      );
    };
  }

  async function recoverProjection(threadId, generation, sourceStream = null) {
    if (
      generation !== app.generation
      || threadId !== app.selectedThreadId
      || (sourceStream && app.stream !== sourceStream)
    ) return;

    if (app.stream) app.stream.close();
    app.stream = null;
    if (app.reconnectTimer) clearTimeout(app.reconnectTimer);
    app.reconnectTimer = null;
    setConnection("", "Refreshing thread snapshot…");

    try {
      const subscribed = await snapshotThenSubscribe({
        state: app.threadState,
        threadId,
        loadSnapshot: (id) => api(`/v1/threads/${encodeURIComponent(id)}`),
        subscribe: (id, sequence) => connectStream(id, sequence, generation),
        isCurrent: () => generation === app.generation && threadId === app.selectedThreadId,
      });
      if (!subscribed) return;
      renderAll();
      showStatus("");
      setConnection("ready", "Local runtime connected");
    } catch (error) {
      if (generation !== app.generation || threadId !== app.selectedThreadId) return;
      showStatus(`Could not refresh the thread snapshot: ${error.message}`);
      setConnection("error", "Runtime recovery failed");
      app.reconnectTimer = setTimeout(
        () => recoverProjection(threadId, generation),
        900,
      );
    }
  }

  function renderAll(preserveScroll = false) {
    renderHeader();
    renderTranscript(preserveScroll);
    renderAttention();
    renderComposer();
    renderThreadList();
  }

  function renderHeader() {
    const thread = app.threadState.thread;
    const summary = app.summaries.find((item) => item.id === app.selectedThreadId);
    const title = thread?.title || summary?.title || (thread ? "New thread" : "Choose a thread");
    setSafeText(dom.title, title);
    setSafeText(dom.kicker, thread ? "Local Runtime thread" : "Local Runtime");
    dom.rename.disabled = !thread;
    dom.archive.disabled = !thread;
    dom.facts.replaceChildren();
    if (!thread) return;

    const workspace = summary?.workspace || thread.workspace || app.workspace?.workspace;
    const branch = summary?.branch || app.workspace?.branch;
    dom.facts.append(factChip("Workspace", basename(workspace) || "local"));
    if (branch) dom.facts.append(factChip("Branch", branch));
    dom.facts.append(factChip("Model", thread.model || "Runtime default"));
    dom.facts.append(factChip("Mode", modeLabel(thread.mode)));
    dom.facts.append(factChip("Permission", permissionLabel(thread)));
  }

  function factChip(label, value) {
    const chip = element("span", "fact-chip");
    chip.append(element("span", "", label));
    chip.append(element("strong", "", value));
    return chip;
  }

  function renderTranscript(preserveScroll) {
    const wasNearBottom = dom.transcript.scrollHeight - dom.transcript.scrollTop - dom.transcript.clientHeight < 120;
    dom.transcript.replaceChildren();
    if (!app.threadState.thread) {
      dom.transcript.append(emptyState("Your local agent, in the browser.", "Create a thread or choose one from the rail. This client uses the same Runtime as the terminal."));
      return;
    }
    if (app.threadState.itemOrder.length === 0) {
      dom.transcript.append(emptyState("Ready for a task.", "Send a message below. Model, mode, and permission posture come from the Runtime and are shown read-only above."));
      return;
    }
    for (const itemId of app.threadState.itemOrder) {
      const item = app.threadState.items.get(itemId);
      if (!item) continue;
      dom.transcript.append(renderItem(item));
    }
    if (!preserveScroll || wasNearBottom) {
      requestAnimationFrame(() => {
        dom.transcript.scrollTop = dom.transcript.scrollHeight;
      });
    }
  }

  function emptyState(title, description) {
    const empty = element("div", "empty-state");
    empty.append(element("div", "empty-orbit", "◌"));
    empty.append(element("h2", "", title));
    empty.append(element("p", "", description));
    return empty;
  }

  function renderItem(item) {
    const detail = item.detail || item.summary || "";
    if (item.kind === "user_message" || item.kind === "agent_message") {
      const role = item.kind === "user_message" ? "user" : "agent";
      const card = element("article", `message ${role} ${item.status === "in_progress" ? "in-progress" : ""}`.trim());
      card.append(element("div", "message-label", role === "user" ? "You" : "Codewhale"));
      card.append(element("div", "message-body", detail));
      return card;
    }
    if (item.kind === "agent_reasoning") {
      const reasoning = element("article", "reasoning");
      const disclosure = element("details");
      disclosure.append(element("summary", "", item.status === "in_progress" ? "Reasoning…" : "Reasoning"));
      disclosure.append(element("pre", "", detail));
      reasoning.append(disclosure);
      return reasoning;
    }

    const receipt = element("article", `receipt ${item.status === "failed" ? "failed" : ""}`.trim());
    receipt.append(element("div", "receipt-label", `${humanize(item.kind)} · ${humanize(item.status)}`));
    receipt.append(element("div", "receipt-summary", item.summary || detail || humanize(item.kind)));
    if (detail && detail !== item.summary) {
      const disclosure = element("details");
      disclosure.append(element("summary", "", "Show receipt"));
      disclosure.append(element("pre", "", detail));
      receipt.append(disclosure);
    }
    return receipt;
  }

  function renderAttention() {
    dom.attention.replaceChildren();
    for (const [approvalId, approval] of app.threadState.approvals) {
      dom.attention.append(renderApproval(approvalId, approval));
    }
    for (const [inputId, input] of app.threadState.userInputs) {
      dom.attention.append(renderUserInput(inputId, input));
    }
    dom.attention.hidden = dom.attention.childElementCount === 0;
  }

  function renderApproval(approvalId, approval) {
    const card = element("article", "attention-card");
    card.append(element("p", "eyebrow", "Approval required"));
    card.append(element("h2", "", approval.tool_name || "Tool request"));
    card.append(element("p", "", approval.intent_summary || approval.description || "Codewhale is waiting for permission."));
    const actions = element("div", "attention-actions");
    const rememberLabel = element("label", "remember-field");
    const remember = document.createElement("input");
    remember.type = "checkbox";
    rememberLabel.append(remember, document.createTextNode("Remember for this thread"));
    const deny = element("button", "quiet-button danger", "Deny");
    deny.type = "button";
    deny.addEventListener("click", () => resolveApproval(approvalId, "deny", remember.checked));
    const allow = element("button", "primary-button", "Allow");
    allow.type = "button";
    allow.addEventListener("click", () => resolveApproval(approvalId, "allow", remember.checked));
    actions.append(rememberLabel, deny, allow);
    card.append(actions);
    return card;
  }

  async function resolveApproval(approvalId, decision, remember) {
    try {
      await api(`/v1/approvals/${encodeURIComponent(approvalId)}`, {
        method: "POST",
        body: JSON.stringify({ decision, remember }),
      });
      app.threadState.approvals.delete(approvalId);
      renderAttention();
    } catch (error) {
      showStatus(error.message);
    }
  }

  function renderUserInput(inputId, envelope) {
    const card = element("form", "attention-card");
    card.append(element("p", "eyebrow", "Input required"));
    card.append(element("h2", "", "Codewhale has a question"));
    const questions = Array.isArray(envelope.request?.questions) ? envelope.request.questions : [];
    const groups = [];
    for (const question of questions) {
      const fieldset = element("fieldset", "question-fieldset");
      fieldset.append(element("legend", "", question.question || question.header || "Choose an option"));
      const controls = [];
      for (const option of Array.isArray(question.options) ? question.options : []) {
        const label = element("label", "answer-option");
        const input = document.createElement("input");
        input.type = question.multi_select ? "checkbox" : "radio";
        input.name = `question-${inputId}-${question.id}`;
        input.value = option.label || "";
        label.append(input);
        const copy = element("span", "", option.label || "Option");
        if (option.description) copy.append(element("small", "", option.description));
        label.append(copy);
        fieldset.append(label);
        controls.push({ input, label: option.label || "", value: option.label || "" });
      }
      let other = null;
      if (question.allow_free_text) {
        other = document.createElement("input");
        other.className = "other-answer";
        other.type = "text";
        other.placeholder = "Other response";
        other.setAttribute("aria-label", `${question.header || "Question"} other response`);
        fieldset.append(other);
      }
      card.append(fieldset);
      groups.push({ question, controls, other });
    }
    const actions = element("div", "attention-actions");
    const submit = element("button", "primary-button", "Submit answers");
    submit.type = "submit";
    actions.append(submit);
    card.append(actions);
    card.addEventListener("submit", async (event) => {
      event.preventDefault();
      const answers = [];
      for (const group of groups) {
        for (const control of group.controls) {
          if (control.input.checked) {
            answers.push({ id: group.question.id, label: control.label, value: control.value });
          }
        }
        const otherValue = group.other?.value.trim();
        if (otherValue) answers.push({ id: group.question.id, label: "Other", value: otherValue });
        if (!answers.some((answer) => answer.id === group.question.id)) {
          showStatus(`Choose an answer for ${group.question.header || group.question.question || "each question"}.`);
          return;
        }
      }
      try {
        await api(`/v1/user-input/${encodeURIComponent(app.selectedThreadId)}/${encodeURIComponent(inputId)}`, {
          method: "POST",
          body: JSON.stringify({ answers }),
        });
        app.threadState.userInputs.delete(inputId);
        showStatus("");
        renderAttention();
      } catch (error) {
        showStatus(error.message);
      }
    });
    return card;
  }

  function latestTurn() {
    const id = app.threadState.turnOrder.at(-1);
    return id ? app.threadState.turns.get(id) : null;
  }

  function activeTurn() {
    const turn = latestTurn();
    return turn && (turn.status === "in_progress" || turn.status === "queued") ? turn : null;
  }

  function renderComposer() {
    const ready = Boolean(app.threadState.thread);
    const active = activeTurn();
    dom.composerInput.disabled = !ready;
    dom.send.disabled = !ready || !dom.composerInput.value.trim();
    dom.interrupt.hidden = !active;
    setSafeText(dom.send, active ? "Steer" : "Send");
  }

  async function createThread() {
    showStatus("");
    try {
      const thread = await api("/v1/threads", { method: "POST", body: "{}" });
      await loadThreads("");
      await selectThread(thread.id);
      dom.composerInput.focus();
      return thread;
    } catch (error) {
      showStatus(error.message);
      return null;
    }
  }

  async function sendMessage() {
    const prompt = dom.composerInput.value.trim();
    if (!prompt) return;
    let threadId = app.selectedThreadId;
    if (!threadId) {
      const thread = await createThread();
      if (!thread) return;
      threadId = thread.id;
    }
    const turn = activeTurn();
    dom.send.disabled = true;
    showStatus("");
    try {
      if (turn) {
        await api(`/v1/threads/${encodeURIComponent(threadId)}/turns/${encodeURIComponent(turn.id)}/steer`, {
          method: "POST",
          body: JSON.stringify({ prompt }),
        });
      } else {
        await api(`/v1/threads/${encodeURIComponent(threadId)}/turns`, {
          method: "POST",
          body: JSON.stringify({ prompt }),
        });
      }
      saveDraft(app.drafts, threadId, "");
      dom.composerInput.value = "";
      resizeComposer();
      renderComposer();
      loadThreads().catch((error) => showStatus(error.message));
    } catch (error) {
      showStatus(error.message);
      renderComposer();
    }
  }

  async function interruptTurn() {
    const turn = activeTurn();
    if (!turn || !app.selectedThreadId) return;
    dom.interrupt.disabled = true;
    try {
      await api(`/v1/threads/${encodeURIComponent(app.selectedThreadId)}/turns/${encodeURIComponent(turn.id)}/interrupt`, { method: "POST" });
    } catch (error) {
      showStatus(error.message);
    } finally {
      dom.interrupt.disabled = false;
    }
  }

  async function archiveThread() {
    if (!app.selectedThreadId) return;
    if (!globalThis.confirm("Archive this thread? You can still access it through the Runtime API.")) return;
    try {
      await api(`/v1/threads/${encodeURIComponent(app.selectedThreadId)}`, {
        method: "PATCH",
        body: JSON.stringify({ archived: true }),
      });
      saveDraft(app.drafts, app.selectedThreadId, "");
      stopStream();
      app.selectedThreadId = "";
      app.threadState = createThreadState();
      await loadThreads();
      if (app.summaries[0]) await selectThread(app.summaries[0].id);
      else renderAll();
    } catch (error) {
      showStatus(error.message);
    }
  }

  function openRenameDialog() {
    if (!app.threadState.thread) return;
    dom.renameInput.value = app.threadState.thread.title || "";
    dom.renameDialog.showModal();
    dom.renameInput.focus();
    dom.renameInput.select();
  }

  async function submitRename(event) {
    event.preventDefault();
    const action = event.submitter?.value;
    if (action !== "save") {
      dom.renameDialog.close();
      return;
    }
    const title = dom.renameInput.value.trim();
    if (!title || !app.selectedThreadId) return;
    try {
      const thread = await api(`/v1/threads/${encodeURIComponent(app.selectedThreadId)}`, {
        method: "PATCH",
        body: JSON.stringify({ title }),
      });
      app.threadState.thread = thread;
      dom.renameDialog.close();
      await loadThreads();
      renderHeader();
    } catch (error) {
      showStatus(error.message);
    }
  }

  function resizeComposer() {
    dom.composerInput.style.height = "auto";
    dom.composerInput.style.height = `${Math.min(dom.composerInput.scrollHeight, 180)}px`;
  }

  function closeRailIfNarrow() {
    if (globalThis.matchMedia("(max-width: 800px)").matches) closeRail();
  }

  dom.railOpen.addEventListener("click", () => dom.shell.classList.add("rail-visible"));
  dom.railClose.addEventListener("click", closeRail);
  dom.railScrim.addEventListener("click", closeRail);
  dom.newThread.addEventListener("click", createThread);
  dom.rename.addEventListener("click", openRenameDialog);
  dom.archive.addEventListener("click", archiveThread);
  dom.renameForm.addEventListener("submit", submitRename);
  dom.interrupt.addEventListener("click", interruptTurn);
  dom.composer.addEventListener("submit", (event) => {
    event.preventDefault();
    sendMessage();
  });
  dom.composerInput.addEventListener("input", () => {
    saveDraft(app.drafts, app.selectedThreadId, dom.composerInput.value);
    resizeComposer();
    renderComposer();
  });
  dom.composerInput.addEventListener("keydown", (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      sendMessage();
    }
  });
  dom.search.addEventListener("input", () => {
    if (app.searchTimer) clearTimeout(app.searchTimer);
    app.searchTimer = setTimeout(() => {
      loadThreads().catch((error) => showStatus(error.message));
    }, 180);
  });
  globalThis.addEventListener("beforeunload", stopStream);

  async function initialize() {
    try {
      [app.runtimeInfo, app.workspace] = await Promise.all([
        api("/v1/runtime/info"),
        api("/v1/workspace/status"),
      ]);
      setConnection("ready", "Local runtime connected");
      await loadThreads();
      if (app.summaries[0]) await selectThread(app.summaries[0].id);
      else renderAll();
    } catch (error) {
      setConnection("error", "Runtime connection failed");
      showStatus(error.message);
      renderAll();
    }
  }

  initialize();
}

function basename(path) {
  if (!path) return "";
  const normalized = String(path).replaceAll("\\", "/").replace(/\/$/, "");
  return normalized.split("/").at(-1) || normalized;
}

function humanize(value) {
  if (!value) return "Status";
  return String(value)
    .replaceAll("_", " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function modeLabel(mode) {
  if (mode === "agent") return "Act";
  if (mode === "plan") return "Plan";
  if (mode === "operate") return "Operate";
  return humanize(mode || "Runtime default");
}

function permissionLabel(thread) {
  if (thread.trust_mode) return "Full Access";
  if (thread.auto_approve) return "Auto-Review";
  return "Ask";
}

function relativeTime(value) {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return "recent";
  const seconds = Math.max(0, Math.round((Date.now() - timestamp) / 1000));
  if (seconds < 60) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86400)}d`;
}

if (typeof document !== "undefined") {
  startBrowserClient();
}
