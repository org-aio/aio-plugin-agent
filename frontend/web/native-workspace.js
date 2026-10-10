export function fileLocation(path, root) {
  const separator = path.includes("\\") ? "\\" : "/";
  const base = path.endsWith(separator) ? path : path + separator;
  const index = path.lastIndexOf(separator);
  let parent = path.slice(0, index + (index === 0 ? 1 : 0));
  if (/^[a-z]:$/i.test(parent)) {
    parent += separator;
  }
  return {
    child: (name) => base + name,
    parent: path === root ? null : parent,
  };
}

export function fileText(dataBase64) {
  const bytes = Uint8Array.from(atob(dataBase64), (value) =>
    value.charCodeAt(0),
  );
  if (bytes.includes(0)) {
    throw new Error("这是二进制文件，请通过文件管理器查看");
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error("文件不是 UTF-8 文本，请通过文件管理器查看");
  }
}

// 原生工作区操作复用同一个连接；异步响应仅能更新发起操作时的设备和界面。
export class NativeWorkspace {
  constructor(state, dialogs, actions) {
    this.state = state;
    this.dialogs = dialogs;
    this.actions = actions;
  }

  context() {
    const { cloud, epoch, native } = this.state;
    if (!cloud?.connected) {
      throw new Error("请先连接云电脑");
    }
    return {
      cloud,
      id: native?.id,
      current: () => cloud === this.state.cloud && epoch === this.state.epoch,
    };
  }

  async show(panel, path = "", entry = "") {
    await this.actions.navigate({ panel, path, entry });
    await this.restore();
  }

  async restore() {
    const params = this.actions.route();
    const panel = params.get("panel");
    if (!this.state.cloud || !["files", "archives"].includes(panel)) {
      if (
        ["native-files", "native-archives"].includes(this.dialogs.current?.kind)
      ) {
        this.dialogs.close();
      }
      return;
    }
    const context = this.context();
    const path = params.get("path") || this.state.workspace;
    const file = params.get("entry") === "file";
    const data = {
      kind: `native-${panel}`,
      title: panel === "files" ? "项目文件" : "已归档会话",
      path,
      file,
      loading: true,
    };
    await this.dialogs.open(data);
    const epoch = this.dialogs.epoch;
    try {
      if (panel === "archives") {
        const result = await context.cloud.request("thread/list", {
          cwd: this.state.workspace,
          archived: true,
          limit: 50,
          sortKey: "updated_at",
          sortDirection: "desc",
        });
        data.items = result.data;
        data.cursor = result.nextCursor;
      } else if (file) {
        const result = await context.cloud.request("fs/readFile", { path });
        data.content = fileText(result.dataBase64);
        data.parent = fileLocation(path, context.cloud.workspace.path).parent;
      } else {
        const result = await context.cloud.request("fs/readDirectory", {
          path,
        });
        const location = fileLocation(path, context.cloud.workspace.path);
        data.parent = location.parent;
        data.items = result.entries
          .filter((item) => item.isDirectory || item.isFile)
          .sort(
            (a, b) =>
              Number(b.isDirectory) - Number(a.isDirectory) ||
              a.fileName.localeCompare(b.fileName),
          )
          .map((item) => ({ ...item, path: location.child(item.fileName) }));
      }
    } catch (error) {
      data.error = error.message;
    }
    if (context.current() && this.dialogs.epoch === epoch) {
      await this.dialogs.open({ ...data, loading: false });
    }
  }

  async action(name, value, element) {
    if (
      name === "close-dialog" &&
      this.dialogs.current?.kind?.startsWith("native-")
    ) {
      await this.actions.navigate({ panel: "", path: "", entry: "" });
      this.dialogs.close();
      return true;
    }
    if (
      ![
        "native-files",
        "native-file",
        "native-archives",
        "native-more-archives",
        "native-restore",
        "native-rename",
        "native-fork",
        "native-revert",
        "native-compact",
      ].includes(name)
    ) {
      return false;
    }
    if (this.state.busy) {
      return true;
    }
    const context = this.context();
    if (element?.disabled) {
      return true;
    }
    if (element) {
      element.disabled = true;
    }
    const writing = [
      "native-fork",
      "native-restore",
      "native-compact",
    ].includes(name);
    if (writing) {
      this.state.busy = true;
      this.actions.updateStatus();
    }
    const menu = document.querySelector("#conversation-actions");
    if (menu) {
      menu.hidden = true;
      document
        .querySelector('[data-action="more"]')
        ?.setAttribute("aria-expanded", "false");
    }
    try {
      switch (name) {
        case "native-files":
          await this.show("files", value || this.state.workspace);
          break;
        case "native-file":
          await this.show("files", value, "file");
          break;
        case "native-archives":
          await this.show("archives");
          break;
        case "native-more-archives": {
          const data = this.dialogs.current;
          const epoch = this.dialogs.epoch;
          const result = await context.cloud.request("thread/list", {
            cwd: this.state.workspace,
            archived: true,
            limit: 50,
            cursor: data.cursor,
            sortKey: "updated_at",
            sortDirection: "desc",
          });
          if (context.current() && epoch === this.dialogs.epoch) {
            await this.dialogs.open({
              ...data,
              items: [...data.items, ...result.data],
              cursor: result.nextCursor,
            });
          }
          break;
        }
        case "native-restore":
          await context.cloud.request("thread/unarchive", { threadId: value });
          if (context.current()) {
            this.dialogs.close();
            await this.actions.navigate({ panel: "", path: "", entry: "" });
            await this.actions.loadConversations();
            await this.actions.select(value);
          }
          break;
        case "native-rename":
          if (!context.id) {
            throw new Error("请先选择会话");
          }
          await this.dialogs.open({
            kind: "native-rename",
            title: "重命名会话",
            item: {
              id: context.id,
              name: this.state.native.name || this.state.native.preview,
            },
          });
          break;
        case "native-fork": {
          if (!context.id) {
            throw new Error("请先选择会话");
          }
          const result = await context.cloud.request("thread/fork", {
            threadId: context.id,
            ...(value ? { beforeTurnId: value } : {}),
          });
          if (context.current()) {
            await this.actions.loadConversations();
            await this.actions.select(result.thread.id);
          }
          break;
        }
        case "native-revert":
          await this.dialogs.confirmation(
            "回退到这条消息之前？本轮及之后的会话记录将被移除，项目文件的改动会保留。",
            async () => {
              if (!context.current()) {
                throw new Error("会话已切换，请重新选择回退位置");
              }
              if (this.state.busy) {
                throw new Error("请等待当前操作完成");
              }
              this.state.busy = true;
              this.actions.updateStatus();
              try {
                await context.cloud.request("thread/revert", {
                  threadId: context.id,
                  beforeTurnId: value,
                });
                if (context.current()) {
                  await this.actions.select(context.id);
                }
              } finally {
                this.state.busy = false;
                this.actions.updateStatus();
              }
            },
          );
          break;
        case "native-compact":
          if (!context.id) {
            throw new Error("请先选择会话");
          }
          await context.cloud.request("thread/compact/start", {
            threadId: context.id,
          });
          break;
        default:
          throw new Error("不支持的云电脑操作");
      }
      return true;
    } finally {
      if (element) {
        element.disabled = false;
      }
      if (writing) {
        this.state.busy = false;
        this.actions.updateStatus();
      }
    }
  }

  async submit(form) {
    if (form.dataset.form !== "native-rename") {
      return false;
    }
    const context = this.context();
    const epoch = this.dialogs.epoch;
    const fields = new FormData(form);
    const name = String(fields.get("name") || "").trim();
    if (!name) {
      throw new Error("请输入会话名称");
    }
    const button = form.querySelector("[type=submit]");
    if (button.disabled || this.state.busy) {
      return true;
    }
    button.disabled = true;
    this.state.busy = true;
    this.actions.updateStatus();
    try {
      await context.cloud.request("thread/name/set", {
        threadId: form.dataset.id,
        name,
      });
      if (context.current()) {
        if (this.state.native?.id === form.dataset.id) {
          this.state.native.name = name;
        }
        await this.actions.loadConversations();
        await this.actions.redraw();
        if (this.dialogs.epoch === epoch) {
          this.dialogs.close();
        }
      }
    } finally {
      button.disabled = false;
      this.state.busy = false;
      this.actions.updateStatus();
    }
    return true;
  }
}
