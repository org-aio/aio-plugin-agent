import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
const directory = "dist/frontend";
await mkdir("dist", { recursive: true });
await rm(directory, { recursive: true, force: true });
await cp("target/dx/az-agent-frontend/release/web/public", directory, {
  recursive: true,
});
let page = await readFile(`${directory}/index.html`, "utf8");
const module = page.match(/src="\/\.\/assets\/([^"/]+\.js)"/);
if (!module) throw new Error("Dioxus module entry changed");
const path = `${directory}/assets/${module[1]}`;
const code = await readFile(path, "utf8");
const startup = /module_or_path:"\/\.\/assets\/([^"/]+\.wasm)"/;
if (!startup.test(code)) throw new Error("Dioxus Wasm entry changed");
await writeFile(
  path,
  code.replace(
    startup,
    (_, name) => `module_or_path:new URL("./${name}",import.meta.url)`,
  ),
);
page = page.replaceAll("/./assets/", "assets/");
await writeFile(`${directory}/index.html`, page);
if (!page.includes("<head>"))
  throw new Error("Dioxus HTML entry is missing its head");
await writeFile(
  `${directory}/settings.html`,
  page.replace("<head>", '<head><meta name="aio-page" content="settings">'),
);
