// 仅供本地界面验收：真实 Wasm 与正式宿主桥，使用独立的内存数据，不连接账户或模型。
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { resolve, extname } from "node:path";
import { randomUUID } from "node:crypto";
import { pathToFileURL } from "node:url";

const providerId = "11111111-1111-4111-8111-111111111111";
const deviceId = "22222222-2222-4222-8222-222222222222";
const model = "gpt-6";
const endpoint = "https://example.invalid/v1";
function conversation(title) {
  return {
    id: randomUUID(),
    title,
    providerId,
    model,
    spaceId: null,
    workerId: null,
    updatedAt: new Date().toISOString(),
  };
}
function message(role, content, status = "complete") {
  return {
    id: randomUUID(),
    role,
    content,
    status,
    error: null,
    tokens: null,
    sourceId: null,
    memoryStatus: null,
    citations: [],
    route: null,
    matchedNodeIds: [],
    activatedNodeIds: [],
  };
}
function fixtures() {
  const threads = [
    { conversation: conversation("新对话"), messages: [], pendingInput: null },
    {
      conversation: conversation("为项目梳理下一步计划"),
      messages: [
        message("user", "帮我梳理一下这个项目，给出清晰的下一步计划。"),
        message(
          "assistant",
          '已经整理好，可以从这三件事开始。\n\n### 1. 明确当前目标\n先跑通一个完整的使用流程，再逐步补齐边界情况。\n\n### 2. 整理任务\n- 检查现有页面和接口\n- 对齐组件布局与交互状态\n- 验证桌面和移动端体验\n\n```rust\nfn main() {\n    println!("Hello, AIO!");\n}\n```\n\n完成后，你会得到一个可以直接继续迭代的工作区。',
        ),
      ],
      pendingInput: null,
    },
    {
      conversation: conversation(
        "一个很长的对话标题，用来确认侧栏会正确截断而不会挤压主工作区",
      ),
      messages: [],
      pendingInput: null,
    },
  ];
  const calls = [];
  const deadlines = new Map();
  return {
    calls,
    invoke(method, path, body) {
      calls.push({ method, path, body });
      if (path === "/settings")
        return {
          allowedEndpoints: [endpoint],
          providers: [
            {
              id: providerId,
              label: "演示服务",
              endpoint,
              model,
              hasSecret: true,
            },
          ],
          maxPromptChars: 24000,
          memoryAvailable: false,
          webSearch: { enabled: false, hasSecret: false },
        };
      if (path === "/providers/models") return [model, "gpt-6-mini"];
      if (path === "/devices")
        return [
          { id: deviceId, label: "本机", platform: "macOS", status: "online" },
        ];
      if (path === "/conversations") {
        if (method === "GET") return threads.map((t) => t.conversation);
        const created = {
          conversation: {
            ...conversation(body.title),
            providerId: body.providerId,
          },
          messages: [],
          pendingInput: null,
        };
        threads.unshift(created);
        return created.conversation;
      }
      const [, , id, action] = path.split("/");
      const thread = threads.find((t) => t.conversation.id === id);
      if (!thread) throw new Error(`未定义的验收接口：${method} ${path}`);
      if (action === "tasks") return [];
      if (action === "model") {
        Object.assign(thread.conversation, body);
        return thread.conversation;
      }
      if (action === "device") {
        thread.conversation.workerId = body.workerId;
        return thread.conversation;
      }
      if (action === "messages") {
        thread.messages.push(
          message("user", body.content),
          message("assistant", "正在整理…", "generating"),
        );
        deadlines.set(id, Date.now() + 2500);
        return thread;
      }
      if (action === "cancel") {
        thread.messages.at(-1).status = "cancelled";
        deadlines.delete(id);
        return thread;
      }
      if (method === "DELETE") {
        threads.splice(threads.indexOf(thread), 1);
        return null;
      }
      if (deadlines.has(id) && Date.now() >= deadlines.get(id)) {
        Object.assign(thread.messages.at(-1), {
          content:
            "已收到。这是本地界面验收的回复，用于检查消息流、滚动和输入框状态。",
          status: "complete",
        });
        deadlines.delete(id);
      }
      return thread;
    },
  };
}

export async function startConversationPreview(port = 4196) {
  const data = fixtures();
  const assets = resolve("dist/frontend");
  let origin;
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url, origin);
      if (req.headers.host !== new URL(origin).host) {
        res.writeHead(403).end();
        return;
      }
      if (url.pathname === "/invoke" && req.method === "POST") {
        if (req.headers.origin !== origin) {
          res.writeHead(403).end();
          return;
        }
        const chunks = [];
        for await (const chunk of req) chunks.push(chunk);
        const input = JSON.parse(Buffer.concat(chunks));
        const body = input.body.length
          ? JSON.parse(Buffer.from(input.body))
          : null;
        const output = data.invoke(input.method, input.path, body);
        res.writeHead(200, { "content-type": "application/json" }).end(
          JSON.stringify({
            status: 200,
            headers: [],
            body: Array.from(Buffer.from(JSON.stringify(output))),
          }),
        );
        return;
      }
      if (url.pathname === "/") {
        const entry =
          url.searchParams.get("page") === "settings"
            ? "settings.html"
            : "index.html";
        res
          .writeHead(200, { "content-type": "text/html" })
          .end(
            `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>AIO · 对话界面验收</title><body style="margin:0;overflow:hidden"><iframe title="智能体" src="/assets/${entry}" sandbox="allow-scripts allow-forms" style="display:block;width:100vw;height:100dvh;border:0"></iframe><script type="module">import {mountBridge} from '/bridge/host.mjs';mountBridge(document.querySelector('iframe'),async request=>{const r=await fetch('/invoke',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({...request,body:Array.from(request.body)})});return r.json()},{clipboard:true});</script></body></html>`,
          );
        return;
      }
      if (url.pathname === "/favicon.ico") {
        res.writeHead(204).end();
        return;
      }
      const bridge = ["/bridge/guest.js", "/bridge/host.mjs"].includes(
        url.pathname,
      );
      const file = bridge
        ? resolve("sdk/web", url.pathname.split("/").at(-1))
        : resolve(assets, "." + url.pathname.slice(7));
      if (
        !bridge &&
        (!url.pathname.startsWith("/assets/") || !file.startsWith(assets + "/"))
      ) {
        res.writeHead(404).end();
        return;
      }
      let bytes = await readFile(file);
      if (extname(file) === ".html")
        bytes = Buffer.from(
          bytes
            .toString()
            .replace(
              "<head>",
              `<head><base href="${origin}/assets/"><script src="/bridge/guest.js"></script>`,
            ),
        );
      const types = {
        ".html": "text/html",
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".wasm": "application/wasm",
        ".css": "text/css",
        ".woff2": "font/woff2",
        ".otf": "font/otf",
      };
      res
        .writeHead(200, {
          "content-type": types[extname(file)] || "application/octet-stream",
          "access-control-allow-origin": "*",
          "cache-control": "no-store",
        })
        .end(bytes);
    } catch (error) {
      res
        .writeHead(500, { "content-type": "application/json" })
        .end(JSON.stringify({ error: error.message }));
    }
  });
  await new Promise((resolve) => server.listen(port, "127.0.0.1", resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
  return { server, origin, calls: data.calls };
}
if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  const { origin } = await startConversationPreview(
    Number(process.env.PORT || 4196),
  );
  console.log(`${origin} — 仅本地界面验收数据`);
}
