import { request, memory, render, report, cancelRender } from "./transport.js";

const $ = (selector) => document.querySelector(selector);
const encode = (value) =>
  btoa(
    Array.from(new TextEncoder().encode(value), (byte) =>
      String.fromCharCode(byte),
    ).join(""),
  );
const decode = (value) =>
  new TextDecoder("utf-8", { fatal: true }).decode(
    Uint8Array.from(atob(value), (character) => character.charCodeAt(0)),
  );
export class Dialogs {
  constructor(state, actions) {
    this.state = state;
    this.actions = actions;
    this.current = null;
    this.confirm = null;
    this.library = null;
    this.panel = null;
    this.secrets = new Map();
    this.focus = null;
    this.epoch = 0;
  }
  async open(data) {
    const epoch = ++this.epoch;
    this.current = data;
    this.focus ??= document.activeElement;
    await render($("#dialogs"), "dialog", data);
    if (epoch === this.epoch) {
      $(".dx-dialog")
        ?.querySelector("input:not([type=hidden]),textarea,select,button")
        ?.focus();
    }
  }
  close() {
    this.epoch++;
    this.current = null;
    this.confirm = null;
    this.secrets.clear();
    cancelRender($("#dialogs"));
    $("#dialogs").replaceChildren();
    this.focus?.focus();
    this.focus = null;
  }
  async confirmation(description, action) {
    this.confirm = action;
    await this.open({ kind: "confirm", title: "确认操作", description });
  }
  id() {
    return this.state.cloud
      ? this.state.native?.id
      : this.state.thread?.conversation.id;
  }
  async action(name, value, element) {
    const state = this.state;
    switch (name) {
      case "close-dialog":
        this.close();
        return;
      case "confirm": {
        const confirm = this.confirm;
        const epoch = this.epoch;
        if (confirm && !element.disabled) {
          element.disabled = true;
          try {
            await confirm();
            if (epoch === this.epoch) {
              this.close();
            }
          } finally {
            element.disabled = false;
          }
        }
        return;
      }
      case "settings":
        await this.open({
          kind: "settings",
          title: "智能体设置",
          item: state.settings,
        });
        return;
      case "provider":
        await this.open({
          kind: "provider",
          title: value ? "编辑模型服务" : "添加模型服务",
          item: state.settings.providers.find((item) => item.id === value) || {
            endpoint:
              state.settings.allowedEndpoints[0] || "https://api.openai.com/v1",
          },
        });
        return;
      case "load-provider-models": {
        const form = element.closest("form");
        const fields = new FormData(form);
        const endpoint = String(fields.get("endpoint")).trim();
        const secret = String(fields.get("secret"));
        const models = await request("POST", "/providers/models", {
          providerId: form.dataset.id || null,
          endpoint,
          secret: secret || null,
        });
        if (!form.isConnected) {
          return;
        }
        const current = new FormData(form);
        if (
          current.get("endpoint").trim() !== endpoint ||
          current.get("secret") !== secret
        ) {
          throw new Error("地址或密钥已变化，请重新读取模型");
        }
        form.elements.model.replaceChildren(
          ...models.map((model) => new Option(model, model)),
        );
        form.dataset.modelsLoaded = "true";
        return;
      }
      case "web-search":
        await this.open({
          kind: "web-search",
          title: "网页搜索",
          item: state.settings.webSearch,
        });
        return;
      case "delete-provider":
        await this.confirmation(
          "删除模型服务后，使用它的会话需要重新选择模型。",
          async () => {
            await request("DELETE", `/providers/${value}`);
            await this.actions.reload();
          },
        );
        return;
      case "delete-thread":
        await this.confirmation(
          state.cloud ? "归档这条 Codex 会话？" : "删除后无法恢复。",
          async () => {
            const current = this.id() === value;
            if (state.cloud) {
              await state.cloud.request("thread/archive", { threadId: value });
              if (this.id() === value) {
                state.native = null;
              }
            } else {
              await request("DELETE", `/conversations/${value}`);
              if (this.id() === value) {
                state.thread = null;
              }
            }
            if (current && !this.id()) {
              this.actions.invalidate();
              await this.actions.navigate({ thread: "" }, true);
            }
            await this.actions.reload();
          },
        );
        return;
      case "questions":
        await this.open({
          kind: "questions",
          title: "继续任务前，请补充",
          item: state.thread.pendingInput,
        });
        return;
      case "approval": {
        const id = JSON.parse(value);
        const item = state.cloud?.approvals.get(id);
        if (!item) {
          throw new Error("请求已过期");
        }
        await this.open({ kind: "approval", title: "Codex 请求确认", item });
        return;
      }
      case "native-approve": {
        const item = this.current.item;
        const method = item.method;
        let result;
        if (
          [
            "item/commandExecution/requestApproval",
            "item/fileChange/requestApproval",
          ].includes(method)
        ) {
          result = { decision: value };
        } else if (
          ["execCommandApproval", "applyPatchApproval"].includes(method)
        ) {
          result = { decision: value === "accept" ? "approved" : "denied" };
        } else if (method === "mcpServer/elicitation/request") {
          result = {
            action: value === "accept" ? "accept" : "decline",
            content: null,
          };
        } else if (method === "item/permissions/requestApproval") {
          result = {
            permissions:
              value === "accept"
                ? {
                    network: item.params.permissions.network,
                    fileSystem: item.params.permissions.fileSystem,
                  }
                : {},
            scope: "turn",
          };
        } else {
          throw new Error(
            "Codex 返回了未支持的审批类型，可以停止当前任务并查看请求详情",
          );
        }
        state.cloud.respond(item.id, result);
        this.close();
        await this.actions.redraw();
        return;
      }
      case "copy-authorization":
        await window.aioPlugin.copy(this.current.item.params.url);
        element.textContent = "链接已复制";
        return;
      case "tasks":
        await this.tasks();
        return;
      case "cancel-task":
        await request(
          "POST",
          `/conversations/${this.id()}/tasks/${value}/cancel`,
        );
        await this.tasks();
        return;
      case "skills":
      case "refresh-skills":
        await this.skills();
        return;
      case "edit-skill":
        await this.editSkill(value);
        return;
      case "delete-skill": {
        const item = this.current.item;
        await this.confirmation(
          `删除 ${item.path} 并同步到设备？`,
          async () => {
            await request("POST", "/skills", {
              operation: "write",
              path: item.path,
              expected: item.hash,
              content: null,
              executable: false,
            });
            await this.skills();
          },
        );
        return;
      }
      case "resolve-skill":
        await this.confirmation(
          "使用所选版本解决同步冲突？旧文件由设备保留备份。",
          async () => {
            await request("POST", "/skills", JSON.parse(value));
            await this.skills();
          },
        );
        return;
      case "environment":
        await this.environment();
        return;
      case "graph":
        this.focused = value || null;
        await this.graph();
        return;
      case "latest-graph":
        this.focused = null;
        await this.graph();
        return;
      case "refresh-panel":
        if (this.panel === "graph") {
          await this.graph();
        } else {
          await this.environment();
        }
        return;
      case "close-panel":
        this.panel = null;
        $("#panel").hidden = true;
        $("#panel").replaceChildren();
        $(".dx-conversation").dataset.context = "false";
        return;
      case "graph-layout":
        $("#memory-graph").hidden = !$("#memory-graph").hidden;
        $("#graph-list").hidden = !$("#graph-list").hidden;
        return;
      case "graph-zoom": {
        const svg = $("#memory-graph");
        const box = svg.viewBox.baseVal;
        const scale = Number(value);
        svg.setAttribute(
          "viewBox",
          `${box.x} ${box.y} ${box.width / scale} ${box.height / scale}`,
        );
        return;
      }
      case "source":
        await this.source(value);
        return;
      case "entry": {
        const item = await memory("GET", `/nodes/${value}`);
        if (item.kind === "SOURCE") {
          await this.source(value);
          return;
        }
        const sources = await memory("GET", `/nodes/${value}/sources`);
        await this.open({ kind: "entry", title: item.title, item, sources });
        return;
      }
      case "retry-source":
        await memory("POST", `/sources/${value}/retry`);
        await this.source(value);
        return;
      case "reveal-secret": {
        const container = element.closest("[data-secret]");
        const output = container.querySelector("[data-secret-value]");
        if (this.secrets.has(value)) {
          this.secrets.delete(value);
          output.textContent = "••••••••";
          element.textContent = "查看秘密";
          element.setAttribute("aria-label", "查看秘密");
          return;
        }
        const epoch = this.epoch;
        const result = await memory("POST", `/secrets/${value}/reveal`);
        if (epoch !== this.epoch) {
          return;
        }
        const generation = Symbol();
        this.secretTimers ??= new Map();
        this.secretTimers.set(value, generation);
        this.secrets.set(value, result.value);
        output.textContent = result.value;
        element.textContent = "隐藏秘密";
        element.setAttribute("aria-label", "隐藏秘密");
        setTimeout(() => {
          if (
            epoch !== this.epoch ||
            this.secretTimers.get(value) !== generation
          ) {
            return;
          }
          this.secrets.delete(value);
          output.textContent = "••••••••";
          element.textContent = "查看秘密";
          element.setAttribute("aria-label", "查看秘密");
        }, 30000);
        return;
      }
      case "copy-secret":
        if (!this.secrets.has(value)) {
          throw new Error("请先查看秘密");
        }
        await window.aioPlugin.copy(this.secrets.get(value));
        return;
      case "grant":
        await this.open({
          kind: "grant",
          title: "秘密授权",
          item: { id: value },
        });
        return;
      case "spaces":
        await this.spaces();
        return;
      case "remove-member": {
        const id = this.current.item.id;
        await this.confirmation("移除此空间成员？", async () => {
          await memory("DELETE", `/spaces/${id}/members/${value}`);
          await this.spaces(id);
        });
        return;
      }
      case "review": {
        const item = await memory("GET", `/sources/${value}/proposal`);
        for (const entry of item.entries ?? []) {
          if (entry.existingId) {
            entry.current = await memory("GET", `/nodes/${entry.existingId}`);
          }
        }
        await this.open({ kind: "review", title: "核实修订", id: value, item });
        return;
      }
      case "resolve-review": {
        const id = this.current.id;
        const hashes = Object.fromEntries(
          (this.current.item.entries ?? [])
            .filter((entry) => entry.existingId)
            .map((entry) => [entry.existingId, entry.current.version]),
        );
        await memory("POST", `/sources/${id}/resolve`, {
          accept: value === "true",
          versions: hashes,
        });
        await this.source(id);
        return;
      }
      default:
        throw new Error(`未实现的界面操作：${name}`);
    }
  }
  async submit(form) {
    const fields = new FormData(form);
    const value = (name) => String(fields.get(name) ?? "");
    const state = this.state;
    const kind = form.dataset.form;
    const id = form.dataset.id;
    const button = form.querySelector("button[type=submit]");
    if (button?.disabled) {
      return;
    }
    if (button) {
      button.disabled = true;
    }
    try {
      switch (kind) {
        case "provider":
          await request(
            id ? "PUT" : "POST",
            id ? `/providers/${id}` : "/providers",
            {
              label: "",
              endpoint: value("endpoint").trim().replace(/\/+$/u, ""),
              model: value("model"),
              secret: value("secret") || null,
            },
          );
          await this.actions.reload();
          if (!$("#page")) {
            await this.open({
              kind: "settings",
              title: "智能体设置",
              item: state.settings,
            });
            return;
          }
          break;
        case "web-search":
          await request("PUT", "/tools/web-search", {
            enabled: !fields.has("clearSecret") && fields.has("enabled"),
            secret: fields.has("clearSecret") ? "" : value("secret") || null,
          });
          await this.actions.reload();
          break;
        case "workspace":
          await this.actions.changeWorkspace(value("path"));
          break;
        case "questions": {
          const answers = Object.fromEntries(
            this.current.item.questions.map((question) => [
              question.id,
              value(`text:${question.id}`) || value(question.id),
            ]),
          );
          if (Object.values(answers).some((answer) => !answer.trim())) {
            throw new Error("请回答全部问题");
          }
          state.thread = await request(
            "POST",
            `/conversations/${this.id()}/input`,
            { requestId: this.current.item.id, answers },
          );
          await this.actions.redraw();
          break;
        }
        case "native-elicitation": {
          const item = this.current.item;
          const content = {};
          for (const [name, schema] of Object.entries(
            item.params.requestedSchema.properties ?? {},
          )) {
            const raw = value(name);
            const required =
              item.params.requestedSchema.required?.includes(name);
            if (!raw && !required && schema.type !== "boolean") {
              continue;
            }
            if (schema.type === "boolean") {
              content[name] = fields.has(name);
            } else if (schema.type === "number" || schema.type === "integer") {
              content[name] = Number(raw);
              if (
                !Number.isFinite(content[name]) ||
                (schema.type === "integer" && !Number.isInteger(content[name]))
              ) {
                throw new Error("请输入有效数字");
              }
            } else if (schema.type === "array" || schema.type === "object") {
              content[name] = JSON.parse(raw);
            } else {
              content[name] = raw;
            }
          }
          state.cloud.respond(item.id, { action: "accept", content });
          await this.actions.redraw();
          break;
        }
        case "native-input": {
          const item = this.current.item;
          const answers = Object.fromEntries(
            item.params.questions.map((question) => [
              question.id,
              { answers: [value(`text:${question.id}`) || value(question.id)] },
            ]),
          );
          if (
            Object.values(answers).some((answer) => !answer.answers[0].trim())
          ) {
            throw new Error("请回答全部问题");
          }
          state.cloud.respond(item.id, { answers });
          await this.actions.redraw();
          break;
        }
        case "skill":
          await request("POST", "/skills", {
            operation: "write",
            path: value("path"),
            expected: form.dataset.hash || null,
            content: encode(value("content")),
            executable: fields.has("executable"),
          });
          await this.skills(!!$("#page"));
          if (!$("#page")) {
            return;
          }
          break;
        case "grant":
          await memory("PUT", `/secrets/${id}/grants`, {
            userId: value("userId").trim(),
            reveal: fields.has("reveal"),
            manage: fields.has("manage"),
          });
          break;
        case "spaces":
          await memory(id ? "PUT" : "POST", id ? `/spaces/${id}` : "/spaces", {
            title: value("title").trim(),
            modelBinding: value("modelBinding") || null,
          });
          await this.actions.reload();
          break;
        case "member":
          await memory("POST", `/spaces/${id}/members`, {
            userId: value("userId").trim(),
            role: value("role"),
          });
          await this.spaces(id);
          return;
        default:
          throw new Error("表单类型无效");
      }
      this.close();
    } finally {
      if (button?.isConnected) {
        button.disabled = false;
      }
    }
  }
  async change(element) {
    if (element.name === "space") {
      await this.spaces(element.value);
    }
    if (element.name === "path" && element.tagName === "SELECT") {
      await this.editSkill(element.value);
    }
  }
  async tasks() {
    if (this.state.cloud) {
      const tasks = (this.state.native?.turns ?? []).flatMap((turn) =>
        (turn.items ?? [])
          .filter(
            (item) =>
              !["userMessage", "agentMessage", "reasoning"].includes(item.type),
          )
          .map((item) => ({
            id: item.id,
            label: item.type,
            device: "云电脑 Codex",
            state: turn.status,
            result: item,
          })),
      );
      await this.open({ kind: "tasks", title: "Codex 执行记录", tasks });
      return;
    }
    const tasks = this.id()
      ? await request("GET", `/conversations/${this.id()}/tasks`)
      : [];
    await this.open({ kind: "tasks", title: "蜂群任务", tasks });
  }
  async skills(page = false) {
    this.library = await request("POST", "/skills", { operation: "list" });
    if (this.state.cloud) {
      this.library.nativeSkills = (
        await this.state.cloud.request("skills/list", {})
      ).data;
    }
    if ($("#page") && (page || $("meta[name=aio-page]").content === "skills")) {
      await render($("#page"), "skills", this.library);
      return;
    }
    if (!page) {
      await this.open({
        kind: "skills",
        title: "Skill 管理",
        item: this.library,
      });
    }
  }
  async editSkill(path) {
    if (!this.library) {
      await this.skills(true);
    }
    if (!path) {
      await this.open({
        kind: "skill",
        title: "新建 Skill",
        item: {},
        content:
          "---\nname: new-skill\ndescription: 说明技能用途\n---\n\n# 使用说明\n",
      });
      return;
    }
    const result = await request("POST", "/skills", {
      operation: "read",
      path,
    });
    if (typeof result.content !== "string") {
      throw new Error("这是二进制资源，请在设备上编辑");
    }
    const prefix = path.split("/")[0] + "/";
    await this.open({
      kind: "skill",
      title: "管理 Skill",
      item: result.file,
      content: decode(result.content),
      files: this.library.files.filter(
        (file) => file.hash && file.path.startsWith(prefix),
      ),
    });
  }
  async source(id) {
    await this.open({
      kind: "source",
      title: "来源资料",
      item: await memory("GET", `/sources/${id}`),
    });
  }
  async spaces(id = "") {
    if (!this.state.settings.memoryAvailable) {
      throw new Error("请先启用记忆插件");
    }
    this.state.spaces = await memory("GET", "/spaces");
    const item = this.state.spaces.find((space) => space.id === id) || {};
    const members = id ? await memory("GET", `/spaces/${id}/members`) : [];
    await this.open({
      kind: "spaces",
      title: "记忆空间",
      item,
      members,
      spaces: this.state.spaces,
      providers: this.state.settings.providers,
    });
  }
  async environment() {
    const state = this.state;
    this.panel = "environment";
    const conversation = state.thread?.conversation;
    const provider = state.settings.providers.find(
      (item) => item.id === conversation?.providerId,
    );
    const values = {
      运行环境: state.cloud ? "Codex CLI" : "AIO Agent",
      模型服务: state.cloud ? state.native?.modelProvider : provider?.label,
      对话模型: state.cloud ? state.model : conversation?.model,
      记忆空间: state.spaces.find((space) => space.id === conversation?.spaceId)
        ?.title,
      执行设备:
        state.devices.find((device) => device.id === state.target)?.label ||
        "对话",
      项目: state.workspace || "未指定",
      审批: state.permission?.approval || "遵循设备配置",
      沙箱: state.permission?.sandbox || "遵循设备配置",
    };
    await this.showPanel({
      kind: "environment",
      title: "环境信息",
      items: Object.entries(values).map(([label, value]) => ({
        label,
        value: value || "未指定",
      })),
    });
  }
  async graph() {
    const thread = this.state.thread;
    const space = thread?.conversation.spaceId;
    if (!space) {
      throw new Error("当前会话尚未关联记忆空间");
    }
    const message =
      thread.messages.find((item) => item.id === this.focused) ??
      thread.messages.findLast((item) => item.role === "assistant");
    const graph = await memory(
      "POST",
      `/activation?spaceId=${encodeURIComponent(space)}`,
      { nodeIds: (message?.activatedNodeIds ?? []).slice(0, 24) },
    );
    const nodes = (graph.nodes ?? []).map((node, index, all) => ({
      ...node,
      active: message?.activatedNodeIds?.includes(node.id) ?? false,
      matched: message?.matchedNodeIds?.includes(node.id) ?? false,
      x: 300 + 180 * Math.cos((index / all.length) * Math.PI * 2),
      y: 250 + 180 * Math.sin((index / all.length) * Math.PI * 2),
    }));
    const byId = new Map(nodes.map((node) => [node.id, node]));
    const edges = (graph.edges ?? []).flatMap((edge) => {
      const a = byId.get(edge.source),
        b = byId.get(edge.target);
      return a && b ? [{ x1: a.x, y1: a.y, x2: b.x, y2: b.y }] : [];
    });
    this.panel = "graph";
    await this.showPanel({
      kind: "graph",
      title: "知识图谱",
      nodes,
      edges,
      total: graph.total,
      focused: !!this.focused,
    });
  }
  async showPanel(data) {
    $("#panel").setAttribute("aria-label", data.title);
    $("#panel").hidden = false;
    $("#panel").className =
      data.kind === "environment"
        ? "dx-conversation__environment"
        : "dx-conversation__context";
    $(".dx-conversation").dataset.context = String(data.kind !== "environment");
    await render($("#panel"), "memory", data);
  }
  async poll() {
    if (
      this.current?.kind === "tasks" &&
      !document.activeElement?.closest(".dx-dialog button")
    ) {
      await this.tasks();
    }
  }
}
