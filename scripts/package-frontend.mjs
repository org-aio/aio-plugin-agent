import {
  cp,
  mkdir,
  readFile,
  readdir,
  mkdtemp,
  writeFile,
} from "node:fs/promises";
import { join, dirname } from "node:path";
import { homedir, tmpdir } from "node:os";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
const execute = promisify(execFile);
const lock = JSON.parse(await readFile("frontend/style-source.json", "utf8"));
const cargo = process.env.CARGO_HOME || join(homedir(), ".cargo");
let source = process.env.AIO_UI_SOURCE;
// 只复用校验过 revision 的源码缓存，缓存缺失时读取同一固定提交。
if (!source) {
  const root = join(cargo, "git/checkouts");
  for (const name of await readdir(root).catch(() => [])) {
    if (!name.startsWith("dioxus-admin-workbench-")) {
      continue;
    }
    for (const revision of await readdir(join(root, name))) {
      const candidate = join(root, name, revision);
      const result = await execute("git", [
        "-C",
        candidate,
        "rev-parse",
        "HEAD",
      ]).catch(() => null);
      if (result?.stdout.trim() === lock.revision) {
        source = candidate;
        break;
      }
    }
    if (source) {
      break;
    }
  }
}
if (!source) {
  source = await mkdtemp(join(tmpdir(), "aio-agent-style-"));
  await execute("git", ["init", source]);
  await execute("git", [
    "-C",
    source,
    "fetch",
    "--depth=1",
    lock.git,
    lock.revision,
  ]);
  await execute("git", ["-C", source, "checkout", "--detach", "FETCH_HEAD"]);
}
const revision = (
  await execute("git", ["-C", source, "rev-parse", "HEAD"])
).stdout.trim();
if (revision !== lock.revision) {
  throw new Error("共享界面样式版本不匹配");
}
await mkdir("dist/frontend", { recursive: true });
for (const file of lock.files) {
  const output = join("dist/frontend/styles", file);
  await mkdir(dirname(output), { recursive: true });
  await cp(join(source, lock.directory, file), output);
}
await cp("frontend/web", "dist/frontend", { recursive: true });
await writeFile(
  "dist/frontend/ui-build.json",
  JSON.stringify({
    framework: "topcoat",
    version: "0.6.2",
    styles: lock.revision,
  }) + "\n",
);
