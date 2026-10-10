// 此模块只传输 Codex 原生协议，不实现模型调用或工具执行循环。
export class CloudCodex {
  constructor(onEvent, onState) {
    this.onEvent = onEvent;
    this.onState = onState;
    this.pending = new Map();
    this.approvals = new Map();
    this.closed = false;
    this.connected = false;
  }
  async open(device) {
    this.view = await window.aioPlugin.deviceView({
      operation: "open",
      device,
      route: "/",
    });
    if (this.closed) {
      await this.close();
      throw new Error("云电脑连接已取消");
    }
    const url = new URL("__channel", this.view.src);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(url);
    this.socket = socket;
    return new Promise((resolve, reject) => {
      const fail = (error) => {
        clearTimeout(timeout);
        this.connected = false;
        reject(error);
        for (const pending of this.pending.values()) {
          clearTimeout(pending.timeout);
          pending.reject(error);
        }
        this.pending.clear();
        this.approvals.clear();
        if (!this.closed) {
          this.onState(error);
          void this.close().catch(this.onState);
        }
      };
      const timeout = setTimeout(
        () => fail(new Error("云电脑连接超时，请重新连接")),
        30000,
      );
      socket.addEventListener("message", (event) => {
        try {
          const packet = JSON.parse(event.data);
          if (packet.kind === "error") {
            throw new Error(packet.error);
          }
          if (packet.kind === "ready") {
            if (packet.snapshot?.runtime !== "codex-cli") {
              throw new Error("此设备运行桌面版，请从 Codex Buddy 页面连接");
            }
            clearTimeout(timeout);
            this.workspace = packet.snapshot.workspace;
            this.connected = true;
            resolve(this.workspace);
            return;
          }
          if (packet.kind !== "frame") {
            return;
          }
          const frame = packet.frame;
          if (frame.kind === "error") {
            throw new Error(frame.error);
          }
          const message = frame.message;
          if (!message) {
            return;
          }
          if (message.id !== undefined && !message.method) {
            const pending = this.pending.get(message.id);
            if (!pending) {
              return;
            }
            this.pending.delete(message.id);
            clearTimeout(pending.timeout);
            if (message.error) {
              pending.reject(
                new Error(message.error.message || "Codex 请求失败"),
              );
            } else {
              pending.resolve(message.result);
            }
            return;
          }
          if (message.id !== undefined) {
            this.approvals.set(message.id, message);
          }
          if (message.method === "serverRequest/resolved") {
            this.approvals.delete(message.params?.requestId);
          }
          if (message.method === "turn/completed") {
            for (const [id, item] of this.approvals) {
              if (item.params?.turnId === message.params?.turn?.id) {
                this.approvals.delete(id);
              }
            }
          }
          this.onEvent(message);
        } catch (error) {
          fail(error);
        }
      });
      const disconnected = () => {
        if (!this.closed) {
          fail(new Error("云电脑连接已断开，请重新连接后核对任务状态"));
        }
      };
      socket.addEventListener("close", disconnected);
      socket.addEventListener("error", disconnected);
    });
  }
  request(method, params = {}) {
    if (!this.connected || this.socket?.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error("云电脑尚未连接"));
    }
    const id = crypto.randomUUID();
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error("Codex 请求超时，请核对结果后继续"));
      }, 40000);
      this.pending.set(id, { resolve, reject, timeout });
      this.socket.send(
        JSON.stringify({ kind: "rpc", message: { id, method, params } }),
      );
    });
  }
  respond(id, result) {
    if (!this.approvals.has(id) || !this.connected) {
      throw new Error("该请求已过期或连接已断开");
    }
    this.socket.send(JSON.stringify({ kind: "rpc", message: { id, result } }));
    this.approvals.delete(id);
  }
  async history(threadId, cursor) {
    const { thread } = await this.request("thread/read", {
      threadId,
      includeTurns: false,
    });
    try {
      const result = await this.request("thread/turns/list", {
        threadId,
        limit: 30,
        sortDirection: "desc",
        itemsView: "full",
        ...(cursor ? { cursor } : {}),
      });
      thread.turns = result.data.reverse();
      thread.earlierCursor = result.nextCursor;
    } catch (error) {
      if (
        !/not supported|unknown variant|not materialized|Method not found/i.test(
          error.message,
        )
      ) {
        throw error;
      }
      try {
        const full = await this.request("thread/read", {
          threadId,
          includeTurns: true,
        });
        thread.turns = full.thread.turns;
      } catch (fallback) {
        // 只有从未写入任务的草稿可以没有历史；真实历史失败必须显示错误。
        if (
          thread.preview ||
          !/not materialized|before first user/i.test(fallback.message)
        ) {
          throw fallback;
        }
        thread.turns = [];
      }
    }
    return thread;
  }
  async close() {
    this.closed = true;
    this.connected = false;
    this.socket?.close();
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timeout);
      pending.reject(new Error("网页已关闭连接"));
    }
    this.pending.clear();
    this.approvals.clear();
    if (this.view) {
      const view = this.view;
      this.view = null;
      await window.aioPlugin.deviceView({ operation: "close", id: view.id });
    }
  }
}

// 完成通知可能只含摘要，合并时保留已经收到的命令输出与用户输入。
export function mergeTurns(before = [], after = []) {
  const turns = before.map((turn) => ({
    ...turn,
    items: [...(turn.items ?? [])],
  }));
  for (const next of after) {
    const index = turns.findIndex((turn) => turn.id === next.id);
    if (index < 0) {
      turns.push(next);
      continue;
    }
    const prior = turns[index];
    const items = [...prior.items];
    for (const item of next.items ?? []) {
      const position = items.findIndex((previous) => previous.id === item.id);
      if (position < 0) {
        items.push(item);
      } else {
        const combined = { ...items[position], ...item };
        for (const field of ["text", "aggregatedOutput", "content"]) {
          if (!item[field]?.length && items[position][field]?.length) {
            combined[field] = items[position][field];
          }
        }
        items[position] = combined;
      }
    }
    turns[index] = { ...prior, ...next, items };
  }
  return turns;
}

export function nativeMessages(thread) {
  return (thread?.turns ?? []).flatMap((turn) =>
    (turn.items ?? []).map((item) => {
      const role =
        item.type === "userMessage"
          ? "user"
          : item.type === "agentMessage"
            ? "assistant"
            : "tool";
      let content = item.text ?? item.aggregatedOutput ?? "";
      if (item.type === "userMessage") {
        content = (item.content ?? [])
          .map((part) => part.text ?? "")
          .join("\n");
      }
      if (item.type === "reasoning") {
        content = (item.summary ?? item.content ?? [])
          .map((part) => (typeof part === "string" ? part : (part.text ?? "")))
          .join("\n");
      }
      if (item.type === "contextCompaction") {
        const labels = {
          inProgress: "正在压缩上下文…",
          completed: "上下文已压缩",
          failed: "上下文压缩失败",
          interrupted: "上下文压缩已停止",
        };
        content = labels[turn.status] ?? "上下文压缩";
      }
      const images =
        (item.content ?? [])
          .filter?.(
            (part) => part.type === "image" && /^https?:/.test(part.url ?? ""),
          )
          .map((part) => part.url) ?? [];
      return {
        id: item.id,
        role,
        content: content || item.command || item.type,
        status: turn.status,
        error: turn.error?.message,
        images,
        details: role === "tool" ? item : undefined,
        turnId: turn.id,
        historyActions:
          role === "user" &&
          turn.status !== "inProgress" &&
          thread.status?.type !== "active" &&
          !thread.turns.some((item) => item.status === "inProgress"),
      };
    }),
  );
}

export function applyEvent(thread, event) {
  const params = event.params ?? {};
  if (!thread?.id || params.threadId !== thread.id) {
    return;
  }
  thread.turns ??= [];
  if (event.method === "turn/started" || event.method === "turn/completed") {
    thread.turns = mergeTurns(thread.turns, [params.turn]);
    thread.status = {
      type: event.method === "turn/completed" ? "idle" : "active",
    };
  }
  if (event.method === "thread/status/changed") {
    thread.status = params.status;
  }
  if (event.method === "thread/name/updated") {
    thread.name = params.threadName;
  }
  let turn = thread.turns.find((turn) => turn.id === params.turnId);
  if (!turn && params.turnId) {
    // 恢复会话会重放历史条目；条目通知本身不能把已完成轮次变成执行中。
    turn = { id: params.turnId, items: [] };
    thread.turns.push(turn);
  }
  if (!turn) {
    return;
  }
  turn.items ??= [];
  if (event.method === "item/started" || event.method === "item/completed") {
    turn.items = mergeTurns(
      [turn],
      [{ id: turn.id, items: [params.item] }],
    )[0].items;
  }
  const fields = {
    "item/agentMessage/delta": "text",
    "item/commandExecution/outputDelta": "aggregatedOutput",
  };
  if (fields[event.method]) {
    let item = turn.items.find((item) => item.id === params.itemId);
    if (!item) {
      item = {
        id: params.itemId,
        type: event.method.includes("agentMessage")
          ? "agentMessage"
          : "commandExecution",
      };
      turn.items.push(item);
    }
    const field = fields[event.method];
    item[field] = (item[field] ?? "") + params.delta;
  }
  if (event.method === "error" && !params.willRetry) {
    turn.error = params.error;
  }
}
