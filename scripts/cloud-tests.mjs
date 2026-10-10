import test from "node:test";
import assert from "node:assert/strict";
import {
  fileLocation,
  fileText,
  NativeWorkspace,
} from "../frontend/web/native-workspace.js";
import {
  CloudCodex,
  applyEvent,
  mergeTurns,
  nativeMessages,
} from "../frontend/web/cloud.js";
import {
  discoverModels,
  modelSelection,
  selectedModel,
} from "../frontend/web/models.js";

test("原生文件浏览保留路径与 UTF-8 内容，不把二进制当文本", () => {
  assert.equal(fileLocation("/project", "/project").parent, null);
  assert.equal(
    fileLocation("/project/目录", "/project").child("a b.txt"),
    "/project/目录/a b.txt",
  );
  assert.equal(fileLocation("/project/目录", "/project").parent, "/project");
  assert.equal(
    fileLocation("C:\\project\\src", "C:\\project").child("readme.md"),
    "C:\\project\\src\\readme.md",
  );
  assert.equal(
    fileText(Buffer.from("中文\n<script>保持原文</script>").toString("base64")),
    "中文\n<script>保持原文</script>",
  );
  assert.throws(() => fileText("AA=="), /二进制/);
  assert.throws(() => fileText("\/w=="), /UTF-8/);
});

test("关闭文件对话框或切换设备后，旧文件响应不能重新打开页面", async () => {
  for (const leave of ["close", "device"]) {
    let finish;
    const state = {
      epoch: 0,
      workspace: "/project",
      cloud: {
        connected: true,
        workspace: { path: "/project" },
        request: () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      },
    };
    const renders = [];
    const dialogs = {
      epoch: 0,
      async open(data) {
        this.epoch++;
        this.current = data;
        renders.push(data);
      },
      close() {
        this.epoch++;
        this.current = null;
      },
    };
    const workspace = new NativeWorkspace(state, dialogs, {
      route: () =>
        new URLSearchParams(
          "panel=files&path=%2Fproject%2Freadme.md&entry=file",
        ),
    });
    const reading = workspace.restore();
    await new Promise(setImmediate);
    if (leave === "close") {
      dialogs.close();
    } else {
      state.epoch++;
    }
    finish({ dataBase64: "aGVsbG8=" });
    await reading;
    assert.equal(renders.length, 1);
    assert.equal(renders[0].loading, true);
  }
});

test("工作区操作不拦截原生审批按钮，执行中重复点击不重复分支", async () => {
  const workspace = new NativeWorkspace({}, {}, {});
  assert.equal(await workspace.action("native-approve", "accept", {}), false);
  const prior = globalThis.document;
  globalThis.document = { querySelector: () => null };
  try {
    let finish;
    let count = 0;
    const state = {
      epoch: 0,
      native: { id: "source" },
      cloud: {
        connected: true,
        request: () => {
          count++;
          return new Promise((resolve) => {
            finish = resolve;
          });
        },
      },
    };
    const actions = {
      updateStatus() {},
      async loadConversations() {},
      async select() {},
    };
    const control = new NativeWorkspace(state, {}, actions);
    const first = control.action("native-fork", "", {});
    await control.action("native-fork", "", {});
    assert.equal(count, 1);
    assert.equal(state.busy, true);
    finish({ thread: { id: "fork" } });
    await first;
    assert.equal(state.busy, false);
  } finally {
    globalThis.document = prior;
  }
});

test("大量供应商不会耗尽宿主桥，慢响应不阻塞其他发现，失败保留原默认模型", async () => {
  const providers = Array.from({ length: 24 }, (_, index) => ({
    id: String(index),
    label: `供应商 ${index}`,
    model: "default",
  }));
  const pending = [];
  const calls = [];
  let active = 0;
  let peak = 0;
  const result = discoverModels(
    providers,
    (provider) => {
      calls.push(provider.id);
      active++;
      peak = Math.max(peak, active);
      return new Promise((resolve, reject) =>
        pending.push(() => {
          active--;
          if (provider.id === "7") {
            reject(new Error("供应商离线"));
          } else {
            resolve(["discovered"]);
          }
        }),
      );
    },
    () => true,
  );
  assert.equal(calls.length, 4);
  // 首个慢请求仍在等待时，其他请求释放的槽位可以继续发现。
  pending.splice(1, 1)[0]();
  await new Promise(setImmediate);
  assert.equal(calls.length, 5);
  providers.reverse();
  while (pending.length) {
    pending.shift()();
    await new Promise(setImmediate);
  }
  const choices = await result;
  assert.equal(calls.length, 24);
  assert.equal(peak, 4);
  assert.equal(choices.length, 47);
  assert.deepEqual(choices.slice(14, 15), [
    { id: '["7","default"]', label: "default" },
  ]);
  assert.equal(choices[0].id, "auto:0");
  assert.equal(choices.at(-1).id, '["23","discovered"]');
});

test("切换设备后旧模型发现不再排队请求剩余供应商", async () => {
  let current = true;
  const pending = [];
  const providers = Array.from({ length: 20 }, (_, id) => ({
    id,
    label: String(id),
  }));
  const result = discoverModels(
    providers,
    () => new Promise((resolve) => pending.push(resolve)),
    () => current,
  );
  assert.equal(pending.length, 4);
  current = false;
  for (const resolve of pending) {
    resolve(["discovered"]);
  }
  await result;
  assert.equal(pending.length, 4);
});

test("首次连接收到全局事件时无需已有会话，也不修改当前会话", () => {
  for (const thread of [null, undefined, { id: "thread", turns: [] }]) {
    applyEvent(thread, {
      method: "serverRequest/resolved",
      params: { requestId: 1 },
    });
    applyEvent(thread, { method: "account/updated", params: {} });
    if (thread) assert.deepEqual(thread, { id: "thread", turns: [] });
  }
});

test("恢复期间重放历史条目不会覆盖已完成轮次的真实状态", () => {
  const events = { id: "thread", turns: [] };
  applyEvent(events, {
    method: "item/completed",
    params: {
      threadId: "thread",
      turnId: "prior",
      item: { id: "reply", type: "agentMessage", text: "已完成" },
    },
  });
  const restored = mergeTurns(
    [{ id: "prior", status: "completed", items: [] }],
    events.turns,
  );
  assert.equal(restored[0].status, "completed");
  assert.equal(restored[0].items[0].text, "已完成");
  applyEvent(events, {
    method: "turn/started",
    params: {
      threadId: "thread",
      turn: { id: "current", status: "inProgress", items: [] },
    },
  });
  assert.equal(events.status.type, "active");
  assert.equal(events.turns.at(-1).status, "inProgress");
});

test("空间默认、服务默认与显式模型往返后语义不变", () => {
  for (const [value, expected] of [
    ["", { providerId: null, model: null }],
    ["auto:provider", { providerId: "provider", model: null }],
    ['["provider","gpt-6"]', { providerId: "provider", model: "gpt-6" }],
  ]) {
    assert.deepEqual(modelSelection(value), expected);
    assert.equal(selectedModel(expected), value);
  }
});

test("完成通知和部分条目不丢失用户输入、工具输出或先前的任务", () => {
  const thread = {
    id: "thread",
    turns: [
      {
        id: "prior",
        status: "completed",
        items: [{ id: "old", type: "agentMessage", text: "已完成的结果" }],
      },
    ],
  };
  applyEvent(thread, {
    method: "turn/started",
    params: {
      threadId: "thread",
      turn: {
        id: "turn",
        status: "inProgress",
        items: [
          {
            id: "user",
            type: "userMessage",
            content: [{ type: "text", text: "读取文件" }],
          },
          {
            id: "command",
            type: "commandExecution",
            command: "cat file",
            aggregatedOutput: "",
          },
        ],
      },
    },
  });
  applyEvent(thread, {
    method: "item/commandExecution/outputDelta",
    params: {
      threadId: "thread",
      turnId: "turn",
      itemId: "command",
      delta: "actual output",
    },
  });
  applyEvent(thread, {
    method: "turn/completed",
    params: {
      threadId: "thread",
      turn: {
        id: "turn",
        status: "completed",
        items: [
          { id: "command", type: "commandExecution", aggregatedOutput: "" },
          { id: "answer", type: "agentMessage", text: "检查完成" },
        ],
      },
    },
  });
  assert.equal(thread.status.type, "idle");
  assert.deepEqual(
    nativeMessages(thread).map((item) => item.content),
    ["已完成的结果", "读取文件", "actual output", "检查完成"],
  );
  applyEvent(thread, {
    method: "turn/started",
    params: { threadId: "other", turn: { id: "unrelated" } },
  });
  assert.equal(thread.turns.length, 2);
});

test("历史从最近一页开始并保留更早消息游标，失败不得变成空历史", async () => {
  const cloud = new CloudCodex(
    () => {},
    () => {},
  );
  const requests = [];
  cloud.request = async (method, params) => {
    requests.push({ method, params });
    return method === "thread/read"
      ? { thread: { id: "t", preview: "saved" } }
      : { data: [{ id: "new" }, { id: "old" }], nextCursor: "earlier" };
  };
  const first = await cloud.history("t");
  assert.deepEqual(
    first.turns.map((turn) => turn.id),
    ["old", "new"],
  );
  assert.equal(first.earlierCursor, "earlier");
  await cloud.history("t", first.earlierCursor);
  assert.equal(requests.at(-1).params.cursor, "earlier");
  cloud.request = async (method, params) => {
    if (method === "thread/read" && !params.includeTurns) {
      return { thread: { id: "t", preview: "saved" } };
    }
    throw new Error("not supported");
  };
  await assert.rejects(cloud.history("t"), /not supported/);
});
