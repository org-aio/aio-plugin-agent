import { request, report } from "./transport.js";
import { nativeMessages, mergeTurns } from "./cloud.js";
import { modelSelection, selectedModel } from "./models.js";
const $ = (selector) => document.querySelector(selector);

// 统一 DOM 事件接线与菜单键盘行为；会话与连接生命周期由 app 持有。
export function bindInteractions({
  state,
  dialogs,
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
}) {
  let composing = false;
  window.addEventListener("pagehide", () => {
    void state.cloud?.close();
  });
  document.addEventListener("click", (event) => {
    const reference = event.target.closest('a[href^="#memory:"]');
    if (reference) {
      event.preventDefault();
      void run(() =>
        dialogs.action("entry", reference.getAttribute("href").slice(8)),
      );
      return;
    }
    if (!event.target.closest(".dx-select")) {
      closeMenus();
    }
    const action = event.target.closest("[data-action]");
    if (!action) {
      return;
    }
    void run(async () => {
      const { action: name, value } = action.dataset;
      if (
        state.busy &&
        [
          "new",
          "select",
          "choose",
          "model",
          "delete-thread",
          "add-workspace",
        ].includes(name)
      )
        return;
      if (name === "more") {
        const menu = $("#conversation-actions");
        menu.hidden = !menu.hidden;
        action.setAttribute("aria-expanded", String(!menu.hidden));
        return;
      }
      if (name === "menu-toggle") {
        const menu = action.closest(".dx-select");
        const open = menu.dataset.state !== "open";
        closeMenus();
        menu.dataset.state = open ? "open" : "closed";
        menu.querySelector("[role=listbox]").hidden = !open;
        action.setAttribute("aria-expanded", String(open));
        if (open) {
          menu.querySelector("[role=option][aria-selected=true]")?.focus();
        }
        return;
      }
      if (name === "choose") {
        if (action.dataset.disabled === "true") {
          return;
        }
        closeMenus();
        await selection(action.dataset.field, value);
        return;
      }

      if (name === "reconnect") {
        const params = route();
        await changeTarget(state.target, { save: false });
        if (params.get("workspace")) {
          state.workspace = params.get("workspace");
        }
        if (params.get("thread")) {
          await select(params.get("thread"), { save: false });
        }
        $("#error").hidden = true;
        return;
      }
      if (name === "earlier") {
        const epoch = state.epoch;
        const older = await state.cloud.history(
          threadId(),
          state.native.earlierCursor,
        );
        if (epoch !== state.epoch) {
          return;
        }
        state.native.turns = mergeTurns(older.turns, state.native.turns);
        state.native.earlierCursor = older.earlierCursor;
        await drawMessages();
        return;
      }
      if (name === "more-history") {
        await loadConversations(true);
        await drawHistory();
        return;
      }
      if (name === "model") {
        await selection("model", value);
        return;
      }
      if (name === "refresh-models") {
        await loadModels();
        await redraw();
        return;
      }
      if (name === "select") {
        await select(value);
        return;
      }
      if (name === "new") {
        await dialogs.open({
          kind: "new",
          title: "新对话",
          spaces: state.spaces,
          cloud: native(),
          memory: state.settings.memoryAvailable,
        });
        return;
      }
      if (name === "stop") {
        await stop();
        return;
      }
      if (name === "refresh-devices") {
        await loadDevices();
        await redraw();
        return;
      }
      if (name === "sidebar") {
        const root = $(".dx-conversation");
        if (matchMedia("(max-width:700px)").matches) {
          root.dataset.history = String(root.dataset.history !== "true");
        } else {
          root.dataset.sidebarCollapsed = String(
            root.dataset.sidebarCollapsed !== "true",
          );
        }
        const scrim = document.querySelector(".dx-conversation__scrim");
        scrim.hidden = root.dataset.history !== "true";
        return;
      }
      if (name === "search") {
        $("#history-search").hidden = !$("#history-search").hidden;
        if ($("#history-search").hidden) {
          $("#history-search").value = "";
          state.query = "";
          await drawHistory();
        } else {
          $("#history-search").focus();
        }
        return;
      }
      if (name === "copy-message") {
        const messages = native()
          ? nativeMessages(state.native)
          : state.thread.messages;
        await window.aioPlugin.copy(
          messages.find((item) => item.id === value)?.content || "",
        );
        action.textContent = "已复制回复";
        action.setAttribute("aria-label", "已复制回复");
        return;
      }
      if (name === "add-workspace") {
        if (native()) {
          await dialogs.open({
            kind: "workspace",
            title: "选择云电脑项目",
            item: { path: state.workspace },
          });
        } else {
          const workspace = await request(
            "POST",
            `/devices/${state.target}/workspaces`,
          );
          await workspaces();
          await changeWorkspace(workspace.id);
        }
        return;
      }
      if ($("#conversation-actions")) {
        $("#conversation-actions").hidden = true;
      }
      await dialogs.action(name, value, action);
    });
  });
  document.addEventListener("submit", (event) => {
    event.preventDefault();
    void run(async () => {
      if (event.target.id === "composer") {
        if (
          state.cloud
            ? state.native?.turns?.some((turn) => turn.status === "inProgress")
            : state.thread?.messages.some(
                (message) => message.status === "generating",
              )
        ) {
          await stop();
        } else {
          await send();
        }
        return;
      }
      if (event.target.dataset.form === "new") {
        if (state.busy) {
          return;
        }
        const fields = new FormData(event.target);
        const epoch = dialogs.epoch;
        const button = event.target.querySelector("button[type=submit]");
        state.busy = true;
        button.disabled = true;
        updateStatus();
        try {
          await newThread(fields.get("title"), fields.get("spaceId") || null);
          if (epoch === dialogs.epoch) {
            dialogs.close();
          }
        } finally {
          state.busy = false;
          button.disabled = false;
          updateStatus();
        }
        return;
      }
      await dialogs.submit(event.target);
    });
  });
  async function selection(id, value) {
    if (id === "target") {
      await changeTarget(value);
      return;
    }
    if (id === "workspace") {
      if (value) {
        await changeWorkspace(value);
      }
      return;
    }
    if (id === "model") {
      if (
        !native() &&
        state.thread?.messages.some((item) =>
          ["queued", "generating"].includes(item.status),
        )
      )
        return;
      if (!native() && threadId()) {
        state.thread.conversation = await request(
          "PUT",
          `/conversations/${threadId()}/model`,
          modelSelection(value),
        );
        state.model = selectedModel(state.thread.conversation);
      } else {
        state.model = value;
      }
      await redraw();
    }
  }
  function closeMenus() {
    for (const menu of document.querySelectorAll(
      ".dx-select[data-state=open]",
    )) {
      menu.dataset.state = "closed";
      menu.querySelector("[role=listbox]").hidden = true;
      menu.querySelector("button").setAttribute("aria-expanded", "false");
    }
  }
  document.addEventListener(
    "change",
    (event) => void run(() => dialogs.change(event.target)),
  );
  document.addEventListener("input", (event) => {
    if (event.target.id === "draft") {
      updateStatus();
    }
    if (event.target.id === "history-search") {
      state.query = event.target.value;
      void drawHistory().catch(report);
    }
    if (event.target.id === "skill-search") {
      for (const row of document.querySelectorAll("[data-skill]")) {
        row.hidden = !row.dataset.skill
          .toLocaleLowerCase()
          .includes(event.target.value.toLocaleLowerCase());
      }
    }
  });
  document.addEventListener("compositionstart", () => {
    composing = true;
  });
  document.addEventListener("compositionend", () => {
    composing = false;
  });
  document.addEventListener("keydown", (event) => {
    if (
      event.target.matches("svg [data-action=entry]") &&
      ["Enter", " "].includes(event.key)
    ) {
      event.preventDefault();
      void run(() => dialogs.action("entry", event.target.dataset.value));
    }
    if (event.key === "Escape") {
      if ($("#conversation-actions")) {
        $("#conversation-actions").hidden = true;
      }
      closeMenus();
      document
        .querySelector(".dx-conversation__scrim")
        ?.setAttribute("hidden", "");
      dialogs.close();
      $(".dx-conversation")?.setAttribute("data-history", "false");
    }
    const option = event.target.closest("[role=option]");
    if (option && ["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      const options = [
        ...option.parentElement.querySelectorAll(
          "[role=option]:not([aria-disabled=true])",
        ),
      ];
      const index = options.indexOf(option);
      options[
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? options.length - 1
            : (index + (event.key === "ArrowDown" ? 1 : -1) + options.length) %
              options.length
      ]?.focus();
    }
    if (option && ["Enter", " "].includes(event.key)) {
      event.preventDefault();
      closeMenus();
      void run(() => selection(option.dataset.field, option.dataset.value));
    }
    if (
      event.target.id === "draft" &&
      event.key === "Enter" &&
      !event.shiftKey &&
      !event.isComposing &&
      !composing
    ) {
      event.preventDefault();
      void run(send);
    }
    if (event.key === "Tab" && $(".dx-dialog")) {
      const focusable = [
        ...$(".dx-dialog").querySelectorAll(
          'button,input,select,textarea,[tabindex="0"]',
        ),
      ].filter((item) => !item.disabled && !item.hidden);
      const index = focusable.indexOf(document.activeElement);
      if (event.shiftKey && index <= 0) {
        event.preventDefault();
        focusable.at(-1)?.focus();
      } else if (!event.shiftKey && index === focusable.length - 1) {
        event.preventDefault();
        focusable[0]?.focus();
      }
    }
  });
}
