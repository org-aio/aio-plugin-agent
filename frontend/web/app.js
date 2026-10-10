import { bindInteractions } from "./interactions.js";
import { request, render, report, cancelRender } from "./transport.js";
import { CloudCodex, nativeMessages, applyEvent, mergeTurns } from "./cloud.js";
import { Dialogs } from "./dialogs.js";
import { discoverModels, modelSelection, selectedModel } from "./models.js";
import { NativeWorkspace } from "./native-workspace.js";

const $ = (selector) => document.querySelector(selector);
const page = $("meta[name=aio-page]").content;
const state = {
  settings: null,
  spaces: [],
  conversations: [],
  thread: null,
  target: "",
  workspace: "",
  model: "",
  models: [],
  devices: [],
  workspaces: [],
  cloud: null,
  native: null,
  busy: false,
  epoch: 0,
  fragment: "",
  query: "",
  pending: null,
};
const dialogs = new Dialogs(state, {
  reload,
  select,
  redraw,
  changeWorkspace,
  stop,
  report,
  navigate,
  invalidate,
});
const nativeWorkspace = new NativeWorkspace(state, dialogs, {
  navigate,
  route,
  loadConversations,
  select,
  redraw,
  updateStatus,
});
let renderTimer;
let messageDrawing = false;
let messageDirty = false;
let pollTimer;
let navigating = false;
let followRoute = false;
const native = () => !!state.cloud;
const threadId = () =>
  native() ? state.native?.id : state.thread?.conversation.id;
const running = () =>
  native()
    ? state.native?.status?.type === "active" ||
      state.native?.turns?.some((turn) => turn.status === "inProgress")
    : state.thread?.messages.some((message) => message.status === "generating");
const run = async (action) => {
  try {
    await action();
  } catch (error) {
    report(error);
  }
};
function route() {
  return new URLSearchParams(state.fragment.replace(/^#/, ""));
}
async function navigate(updates, replace = false) {
  const params = route();
  for (const [key, value] of Object.entries(updates)) {
    if (value) {
      params.set(key, value);
    } else {
      params.delete(key);
    }
  }
  const next = "#" + params;
  if (next === state.fragment) {
    return;
  }
  navigating = true;
  state.fragment = next;
  try {
    await window.aioPlugin.navigate(next, { replace });
  } finally {
    navigating = false;
  }
}
function invalidate() {
  state.epoch++;
  clearTimeout(renderTimer);
  renderTimer = null;
  messageDirty = false;
  cancelRender($("#messages"));
  cancelRender($("#history"));
  cancelRender($("#controls"));
}
async function loadSettings() {
  state.settings = await request("GET", "/settings");
  if (state.settings.memoryAvailable) {
    try {
      state.spaces = await request("POST", "/memory", {
        method: "GET",
        path: "/spaces",
        body: null,
      });
    } catch (error) {
      report(error);
    }
  }
}
async function loadModels() {
  const epoch = state.epoch;
  if (native()) {
    const result = await state.cloud.request("model/list", { limit: 100 });
    if (epoch !== state.epoch) {
      return;
    }
    state.models = result.data.map((model) => ({
      id: model.model,
      label: model.displayName || model.model,
    }));
    if (!state.model) {
      state.model =
        result.data.find((model) => model.isDefault)?.model ||
        result.data[0]?.model ||
        "";
    }
    return;
  }
  const models = await discoverModels(
    state.settings.providers,
    (provider) =>
      request("POST", "/providers/models", {
        providerId: provider.id,
        endpoint: provider.endpoint,
        secret: null,
      }),
    () => epoch === state.epoch,
  );
  if (epoch !== state.epoch) {
    return;
  }
  state.models = models;
}
async function loadDevices() {
  const [workers, views] = await Promise.allSettled([
    request("GET", "/devices"),
    window.aioPlugin.deviceView?.({ operation: "list" }) ?? Promise.resolve([]),
  ]);
  const list = workers.status === "fulfilled" ? workers.value : [];
  const cloud =
    views.status === "fulfilled"
      ? views.value.filter((device) =>
          device.capabilities.includes("codex.cli"),
        )
      : [];
  state.devices = [
    ...cloud.map((device) => ({
      ...device,
      cloud: true,
      label: `云电脑 · ${device.label}`,
    })),
    ...list.filter((device) => !cloud.some((item) => item.id === device.id)),
  ];
  if (workers.status === "rejected" && views.status === "rejected") {
    throw workers.reason;
  }
}
async function loadConversations(more = false) {
  const epoch = state.epoch;
  if (native()) {
    const result = await state.cloud.request("thread/list", {
      cwd: state.workspace,
      limit: 50,
      sortKey: "updated_at",
      sortDirection: "desc",
      ...(more && state.cursor ? { cursor: state.cursor } : {}),
    });
    if (epoch !== state.epoch) {
      return;
    }
    const items = result.data.map((thread) => ({
      id: thread.id,
      title: thread.name || thread.preview || "新对话",
    }));
    state.conversations = more
      ? [
          ...state.conversations,
          ...items.filter(
            (item) =>
              !state.conversations.some((previous) => previous.id === item.id),
          ),
        ]
      : items;
    state.cursor = result.nextCursor;
    // 原生未提交新轮次的分支可能尚未进入索引，保留当前真实打开的会话入口。
    if (
      state.native &&
      !state.conversations.some((item) => item.id === state.native.id)
    ) {
      state.conversations.unshift({
        id: state.native.id,
        title: state.native.name || state.native.preview || "新对话",
      });
    }
  } else {
    const result = await request("GET", "/conversations");
    if (epoch === state.epoch) {
      state.conversations = result;
      state.cursor = null;
    }
  }
}
async function reload() {
  await loadSettings();
  if (page === "settings") {
    await render($("#page"), "settings", state.settings);
    return;
  }
  if (page === "skills") {
    await dialogs.skills(true);
    return;
  }
  await Promise.all([loadDevices(), loadModels(), loadConversations()]);
  await redraw();
}
async function select(id, { save = true } = {}) {
  invalidate();
  const epoch = state.epoch;
  const client = state.cloud;
  $("#draft").value = "";
  state.pending = null;
  if (client) {
    state.native = { id, turns: [] };
    const resumed = await client.request("thread/resume", {
      threadId: id,
    });
    if (epoch !== state.epoch) {
      return;
    }
    const thread = await client.history(id);
    if (epoch !== state.epoch) {
      return;
    }
    const events = state.native;
    state.native = {
      ...resumed.thread,
      ...thread,
      status: events.status ?? resumed.thread.status,
      turns: mergeTurns(thread.turns, events.turns),
    };
    state.permission = {
      approval: resumed.approvalPolicy,
      sandbox: resumed.sandbox?.type,
    };
    state.workspace = resumed.thread.cwd;
    state.model = resumed.model || state.model;
    if (!state.conversations.some((item) => item.id === id)) {
      state.conversations.unshift({
        id,
        title: state.native.name || state.native.preview || "新对话",
      });
    }
    await workspaces();
  } else {
    const thread = await request("GET", `/conversations/${id}`);
    if (epoch !== state.epoch) {
      return;
    }
    state.thread = thread;
    state.target = thread.conversation.workerId || "";
    state.workspace = thread.conversation.workspaceId || "";
    state.model = selectedModel(thread.conversation);
    await workspaces();
  }
  if (save) {
    await navigate({
      thread: id,
      target: state.target,
      workspace: native() ? state.workspace : "",
    });
  }
  await redraw();
}
async function workspaces() {
  if (native()) {
    const epoch = state.epoch;
    const result = await state.cloud.request("workspace/list", {
      cwd: state.workspace,
    });
    if (epoch !== state.epoch) {
      return;
    }
    state.workspaces = [
      {
        id: result.path,
        label: result.path.split("/").filter(Boolean).at(-1) || "项目",
      },
      ...(result.parent
        ? [{ id: result.parent, label: "上一级项目目录" }]
        : []),
      ...result.entries,
    ];
    return;
  }
  state.workspaces = state.target
    ? await request("GET", `/devices/${state.target}/workspaces`)
    : [];
}
async function changeWorkspace(value) {
  if (native()) {
    // thread/list 在设备上验证目录，成功后才切换；不能把既有会话搬到别的目录。
    await state.cloud.request("thread/list", { cwd: value, limit: 1 });
    invalidate();
    state.workspace = value;
    state.native = null;
    await workspaces();
    await loadConversations();
    await navigate({ workspace: value, thread: "" });
  } else if (threadId()) {
    state.thread.conversation = await request(
      "PUT",
      `/conversations/${threadId()}/workspace`,
      { workspaceId: value || null },
    );
    state.workspace = value;
  }
  await redraw();
}
async function changeTarget(id, { save = true } = {}) {
  invalidate();
  const epoch = state.epoch;
  const old = state.cloud;
  const previousModel = state.model;
  state.cloud = null;
  state.native = null;
  state.target = id;
  state.model = "";
  if (old) {
    await old.close();
  }
  const device = state.devices.find((item) => item.id === id);
  if (device?.cloud) {
    const cloud = new CloudCodex(
      (event) => {
        if (state.cloud !== cloud) {
          return;
        }
        applyEvent(state.native, event);
        scheduleDraw();
        if (event.method === "turn/completed") {
          void loadConversations().then(drawHistory).catch(report);
        }
      },
      (error) => {
        if (state.cloud === cloud) {
          report(error);
          updateStatus();
        }
      },
    );
    state.cloud = cloud;
    const workspace = await cloud.open(id);
    if (epoch !== state.epoch) {
      await cloud.close();
      return;
    }
    state.workspace = workspace.path;
    state.thread = null;
  } else {
    state.model = old ? "" : previousModel;
    state.workspace = "";
    if (state.thread) {
      state.thread.conversation = await request(
        "PUT",
        `/conversations/${threadId()}/device`,
        { workerId: id || null },
      );
    }
  }
  if (epoch !== state.epoch) {
    return;
  }
  await Promise.all([workspaces(), loadModels(), loadConversations()]);
  if (epoch !== state.epoch) {
    return;
  }
  if (save) {
    await navigate({
      target: id,
      thread: native() ? "" : threadId(),
      workspace: native() ? state.workspace : "",
    });
  }
  if (!native() && !state.thread && state.conversations.length) {
    await select(state.conversations[0].id, { save });
  }
  await redraw();
}
async function newThread(title = "新对话", spaceId = null) {
  if (native()) {
    const result = await state.cloud.request("thread/start", {
      cwd: state.workspace,
      model: state.model,
    });
    state.native = result.thread;
    state.native.turns ??= [];
    state.native.name = title;
    state.permission = {
      approval: result.approvalPolicy,
      sandbox: result.sandbox?.type,
    };
    if (title !== "新对话") {
      await state.cloud.request("thread/name/set", {
        threadId: result.thread.id,
        name: title,
      });
    }
  } else {
    const conversation = await request("POST", "/conversations", {
      title,
      spaceId,
      providerId: state.settings.providers[0]?.id ?? null,
    });
    state.thread = await request("GET", `/conversations/${conversation.id}`);
    if (state.target) {
      state.thread.conversation = await request(
        "PUT",
        `/conversations/${conversation.id}/device`,
        { workerId: state.target },
      );
    }
    if (state.model) {
      state.thread.conversation = await request(
        "PUT",
        `/conversations/${conversation.id}/model`,
        modelSelection(state.model),
      );
    }
    if (state.workspace) {
      state.thread.conversation = await request(
        "PUT",
        `/conversations/${conversation.id}/workspace`,
        { workspaceId: state.workspace },
      );
    }
  }
  await loadConversations();
  await navigate({ thread: threadId() });
  await redraw();
}
async function send() {
  if (state.busy || (!native() && state.thread?.pendingInput)) {
    return;
  }
  if (
    running() ||
    (!native() &&
      state.thread?.messages.some((message) => message.status === "queued"))
  ) {
    return;
  }
  const content = $("#draft").value;
  if (!content.trim()) {
    return;
  }
  state.busy = true;
  updateStatus();
  try {
    if (!threadId()) {
      await newThread();
    }
    if (native()) {
      const id = threadId();
      const result = await state.cloud.request("turn/start", {
        threadId: id,
        ...(state.model ? { model: state.model } : {}),
        input: [{ type: "text", text: content, text_elements: [] }],
      });
      if (state.native?.id === id) {
        if (!state.native.turns.some((turn) => turn.id === result.turn.id)) {
          state.native.turns.push(result.turn);
        }
        state.native.status = { type: "active" };
      }
    } else {
      const id = threadId();
      const pending =
        state.pending?.id === id && state.pending.prompt.content === content
          ? state.pending.prompt
          : { requestId: crypto.randomUUID(), content };
      state.pending = { id, prompt: pending };
      await request("POST", `/conversations/${id}/messages`, pending);
      state.thread = await request("GET", `/conversations/${id}`);
      state.pending = null;
    }
    $("#draft").value = "";
    await loadConversations();
    await redraw();
  } finally {
    state.busy = false;
    updateStatus();
  }
}
async function stop() {
  if (native()) {
    const turn = state.native?.turns.findLast(
      (turn) => turn.status === "inProgress",
    );
    if (!turn) {
      throw new Error("当前任务状态尚未恢复，请刷新会话后停止");
    }
    await state.cloud.request("turn/interrupt", {
      threadId: threadId(),
      turnId: turn.id,
    });
  } else if (threadId()) {
    state.thread = await request("POST", `/conversations/${threadId()}/cancel`);
  }
  await redraw();
}
function scheduleDraw() {
  messageDirty = true;
  if (!renderTimer && !messageDrawing) {
    renderTimer = setTimeout(async () => {
      renderTimer = null;
      messageDrawing = true;
      messageDirty = false;
      try {
        await drawMessages();
      } catch (error) {
        report(error);
      } finally {
        messageDrawing = false;
        if (messageDirty) {
          scheduleDraw();
        }
      }
    }, 150);
  }
}
async function drawHistory() {
  const query = state.query.toLocaleLowerCase();
  await render($("#history"), "history", {
    native: native(),
    conversations: state.conversations.filter((item) =>
      item.title.toLocaleLowerCase().includes(query),
    ),
    selected: threadId(),
    spaces: state.spaces,
    more: !!state.cursor,
  });
}
async function drawMessages() {
  const messages = native()
    ? nativeMessages(state.native)
    : (state.thread?.messages ?? []);
  const pendingInput = native() ? null : state.thread?.pendingInput;
  await render($("#messages"), "messages", {
    messages,
    pendingInput,
    earlier: !!state.native?.earlierCursor,
    approvals: [...(state.cloud?.approvals.values() ?? [])].filter(
      (item) => item.params?.threadId === threadId(),
    ),
  });
  $(".dx-conversation__main").dataset.empty = String(!messages.length);
  $("#title").textContent = native()
    ? state.native?.name || state.native?.preview || "云电脑 Codex"
    : state.thread?.conversation.title || "新对话";
  updateStatus();
}
function updateStatus() {
  if (!$("#send")) {
    return;
  }
  const active = running();
  for (const item of document.querySelectorAll("[data-native]")) {
    item.hidden = !native();
  }
  for (const item of document.querySelectorAll("[data-native=thread] button")) {
    item.disabled = !threadId() || state.busy || active;
  }
  const queued =
    !native() &&
    state.thread?.messages.some((message) => message.status === "queued");
  $("#send").textContent = active ? "■" : "↑";
  $("#send").setAttribute("aria-label", active ? "停止" : "发送");
  $("#send").disabled =
    state.busy ||
    queued ||
    (!active && !$("#draft").value.trim()) ||
    (!native() && !!state.thread?.pendingInput) ||
    (native() && !state.cloud.connected);
  $("#draft").disabled = state.busy;
  for (const item of document.querySelectorAll("[data-action=add-workspace]")) {
    item.disabled = !state.target || state.busy;
  }
  const modelLocked =
    state.busy ||
    (!native() && (active || queued || !!state.thread?.pendingInput));
  for (const item of document.querySelectorAll(
    "#model-control button,#model-shortcuts [data-action=model]",
  )) {
    item.disabled = modelLocked;
  }
  for (const item of document.querySelectorAll(
    "[data-action=new],[data-action=select]",
  )) {
    item.disabled = state.busy;
  }
  $("#reconnect").hidden = !native() || state.cloud.connected;
  $("#status").textContent = queued
    ? "正在保存消息…"
    : active
      ? native()
        ? "正在执行…"
        : "正在生成回复…"
      : native()
        ? `Codex · ${state.devices.find((item) => item.id === state.target)?.label ?? "云电脑"}`
        : state.thread?.pendingInput
          ? "等待回答后继续"
          : state.thread?.messages.some((message) =>
                ["pending", "processing"].includes(message.memoryStatus),
              )
            ? "正在整理记忆…"
            : "";
}
async function redraw() {
  if (page !== "chat") {
    return;
  }
  const controls = {
    devices: state.devices,
    target: state.target,
    workspaces: state.workspaces,
    workspace: state.workspace,
    models: state.models,
    model: state.model,
  };
  await Promise.all([
    drawHistory(),
    drawMessages(),
    render($("#controls"), "controls", controls),
    render($("#model-control"), "models", controls),
    render($("#model-shortcuts"), "shortcuts", controls),
  ]);
  updateStatus();
  for (const item of document.querySelectorAll(
    "[data-action=spaces],[data-action=graph]",
  )) {
    item.hidden =
      !state.settings.memoryAvailable ||
      (native() && item.dataset.action === "graph");
  }
}
async function restoreRoute() {
  if (page !== "chat") {
    return;
  }
  const params = route();
  const restoring = state.fragment;
  followRoute = true;
  try {
    const target = params.get("target") || "";
    if (
      target !== state.target ||
      (target &&
        !state.cloud &&
        state.devices.find((item) => item.id === target)?.cloud)
    ) {
      await changeTarget(target, { save: false });
    }
    if (
      native() &&
      params.get("workspace") &&
      params.get("workspace") !== state.workspace
    ) {
      state.workspace = params.get("workspace");
      await workspaces();
      await loadConversations();
    }
    const id = params.get("thread");
    if (id) {
      await select(id, { save: false });
    } else {
      state.native = null;
      state.thread = null;
      await redraw();
    }
    await nativeWorkspace.restore();
  } finally {
    followRoute = false;
    if (state.fragment !== restoring && !navigating) {
      void run(restoreRoute);
    }
  }
}
bindInteractions({
  state,
  dialogs,
  nativeWorkspace,
  native,
  threadId,
  run,
  route,
  changeTarget,
  select,
  drawMessages,
  loadConversations,
  drawHistory,
  loadModels,
  redraw,
  stop,
  loadDevices,
  workspaces,
  changeWorkspace,
  newThread,
  send,
  updateStatus,
});
async function poll() {
  try {
    if (page === "chat" && !native() && threadId() && !state.busy) {
      const epoch = state.epoch;
      const id = threadId();
      const latest = await request("GET", `/conversations/${id}`);
      if (
        epoch === state.epoch &&
        JSON.stringify(latest) !== JSON.stringify(state.thread)
      ) {
        state.thread = latest;
        await drawMessages();
      }
    }
    await dialogs.poll();
  } catch (error) {
    report(error);
  } finally {
    pollTimer = setTimeout(poll, 1000);
  }
}
await run(async () => {
  if (!window.aioPlugin) {
    throw new Error("请从 AIO 工作空间打开智能体");
  }
  window.aioPlugin.onNavigationChange?.((next) => {
    state.fragment = next;
    if (!navigating && state.settings && !followRoute) {
      void run(restoreRoute);
    }
  });
  await reload();
  if (page === "chat") {
    if (
      !route().get("thread") &&
      !route().get("target") &&
      state.conversations.length
    ) {
      await select(state.conversations[0].id);
    } else {
      await restoreRoute();
    }
  }
  void poll();
});
