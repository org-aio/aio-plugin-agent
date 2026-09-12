import { createServer } from "node:http";
import { readFile, realpath } from "node:fs/promises";
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { resolve, extname, sep } from "node:path";
import { parse, serialize } from "parse5";

const root = process.cwd(),
  port = Number(process.env.PORT || 4192),
  backendPort = Number(process.env.AIO_PLUGIN_PORT || 4193);
const origin = `http://127.0.0.1:${port}`,
  ticket = randomBytes(32).toString("hex");
const config = JSON.parse(
  await readFile(
    process.env.AIO_AGENT_DEV_CONFIG || ".local/runtime.json",
    "utf8",
  ),
);
const assets = await realpath(resolve(root, "dist/frontend"));
const sdk = resolve(root, "sdk/web");
const child =
  process.env.AIO_AGENT_EXTERNAL_BACKEND === "1"
    ? null
    : spawn(resolve(root, "target/debug/az-agent-server"), [], {
        stdio: ["ignore", "inherit", "inherit"],
        env: {
          ...process.env,
          AIO_AGENT_DATABASE_URL: config.databaseUrl,
          AIO_AGENT_MASTER_KEY: config.masterKey,
          AIO_AGENT_INGRESS_TOKEN: config.ingressToken,
          AIO_AGENT_ENDPOINTS: config.allowedEndpoints.join(","),
          AIO_AGENT_ALLOW_LOOPBACK:
            process.env.AIO_AGENT_ALLOW_LOOPBACK ||
            (config.allowLoopback ? "1" : "0"),
          AIO_PLUGIN_PORT: String(backendPort),
        },
      });
let stopped = false;
child?.on("exit", (code) => {
  stopped = true;
  if (code) console.error(`Backend stopped (${code})`);
});
for (let attempt = 0; ; attempt++) {
  if (stopped || attempt > 100)
    throw new Error("Agent backend failed to start");
  try {
    const result = await fetch(`http://127.0.0.1:${backendPort}/health`);
    if (result.ok) break;
  } catch {}
  await new Promise((resolve) => setTimeout(resolve, 100));
}
const shell = `<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>智能体 · AIO</title><body style="margin:0;overflow:hidden"><iframe id="plugin" title="智能体" src="/assets/index.html" sandbox="allow-scripts" style="display:block;width:100vw;height:100vh;border:0"></iframe><script type="module">import {mountBridge} from '/bridge/host.mjs';const dispose=mountBridge(document.getElementById('plugin'),async request=>{const response=await fetch('/invoke',{method:'POST',headers:{'content-type':'application/json','x-aio-ticket':'${ticket}'},body:JSON.stringify({...request,body:Array.from(request.body)})});if(!response.ok)throw new Error(await response.text());return response.json()},{clipboard:true});addEventListener('pagehide',dispose,{once:true});</script></body></html>`;
const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".wasm": "application/wasm",
  ".otf": "font/otf",
};
const server = createServer(async (req, res) => {
  try {
    if (req.headers.host !== `127.0.0.1:${port}`) {
      res.writeHead(403).end();
      return;
    }
    const url = new URL(req.url, origin);
    if (url.pathname === "/invoke" && req.method === "POST") {
      if (
        req.headers.origin !== origin ||
        req.headers["x-aio-ticket"] !== ticket
      ) {
        res.writeHead(403).end();
        return;
      }
      let size = 0;
      const chunks = [];
      for await (const chunk of req) {
        size += chunk.length;
        if (size > 1024 * 1024) {
          res.writeHead(413).end();
          return;
        }
        chunks.push(chunk);
      }
      const input = JSON.parse(Buffer.concat(chunks).toString());
      if (
        !["GET", "POST", "PUT", "DELETE"].includes(input.method) ||
        typeof input.path !== "string" ||
        !/^\/(settings|providers|conversations|memory)(\/[a-zA-Z0-9/-]+)?$/.test(
          input.path,
        ) ||
        !Array.isArray(input.body) ||
        input.body.length > 256 * 1024 ||
        input.body.some((v) => !Number.isInteger(v) || v < 0 || v > 255)
      ) {
        res.writeHead(400).end();
        return;
      }
      const response = await fetch(
        `http://127.0.0.1:${backendPort}${input.path}`,
        {
          method: input.method,
          headers: {
            "content-type": "application/json",
            "x-aio-token": config.ingressToken,
            "x-aio-tenant-id": "preview",
            "x-aio-user-id": "developer",
          },
          body: input.method === "GET" ? undefined : Buffer.from(input.body),
          signal: AbortSignal.timeout(10000),
          redirect: "error",
        },
      );
      const body = new Uint8Array(await response.arrayBuffer());
      res
        .writeHead(200, {
          "content-type": "application/json",
          "cache-control": "no-store",
        })
        .end(
          JSON.stringify({
            status: response.status,
            headers: [
              {
                name: "content-type",
                value:
                  response.headers.get("content-type") || "application/json",
              },
            ],
            body: Array.from(body),
          }),
        );
      return;
    }
    if (req.method !== "GET") {
      res.writeHead(405).end();
      return;
    }
    if (url.pathname === "/") {
      res
        .writeHead(200, {
          "content-type": "text/html; charset=utf-8",
          "cache-control": "no-store",
        })
        .end(shell);
      return;
    }
    if (url.pathname === "/favicon.ico") {
      res.writeHead(204).end();
      return;
    }
    if (["/bridge/guest.js", "/bridge/host.mjs"].includes(url.pathname)) {
      res
        .writeHead(200, {
          "content-type": "text/javascript",
          "access-control-allow-origin": "*",
        })
        .end(await readFile(resolve(sdk, url.pathname.split("/").at(-1))));
      return;
    }
    if (!url.pathname.startsWith("/assets/")) {
      res.writeHead(404).end();
      return;
    }
    const file = await realpath(
      resolve(assets, decodeURIComponent(url.pathname.slice(8))),
    );
    if (!file.startsWith(assets + sep)) {
      res.writeHead(403).end();
      return;
    }
    let bytes = await readFile(file);
    const headers = {
      "content-type": types[extname(file)] || "application/octet-stream",
      "access-control-allow-origin": "*",
      "cache-control": "no-cache",
      "x-content-type-options": "nosniff",
    };
    if (extname(file) === ".html") {
      const doc = parse(bytes.toString());
      const head = doc.childNodes
        .find((n) => n.tagName === "html")
        .childNodes.find((n) => n.tagName === "head");
      head.childNodes.unshift({
        nodeName: "script",
        tagName: "script",
        namespaceURI: "http://www.w3.org/1999/xhtml",
        attrs: [{ name: "src", value: "/bridge/guest.js" }],
        childNodes: [],
        parentNode: head,
      });
      bytes = Buffer.from(serialize(doc));
      headers["content-security-policy"] =
        `sandbox allow-scripts; default-src 'none'; script-src ${origin} 'unsafe-inline' 'wasm-unsafe-eval'; connect-src ${origin}/assets/; img-src ${origin}/assets/ data: blob:; font-src ${origin}/assets/; style-src 'unsafe-inline'; worker-src blob:; base-uri 'none'; form-action 'none'`;
    }
    res.writeHead(200, headers).end(bytes);
  } catch (error) {
    console.error(error.code || error.name);
    if (!res.headersSent) res.writeHead(500);
    res.end("Preview request failed");
  }
});
server.listen(port, "127.0.0.1", () => console.log(origin));
server.on("error", () => child?.kill("SIGTERM"));
for (const signal of ["SIGTERM", "SIGINT"])
  process.on(signal, () => {
    child?.kill("SIGTERM");
    server.close();
  });
