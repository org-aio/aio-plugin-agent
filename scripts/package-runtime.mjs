import { cpSync, rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { resolve, sep } from "node:path";

const tests = resolve("runtime/test");
rmSync("dist/runtime", { recursive: true, force: true });
cpSync("runtime", "dist/runtime", {
  recursive: true,
  filter: (source) =>
    resolve(source) !== tests && !resolve(source).startsWith(tests + sep),
});
for (const file of ["package.json", "package-lock.json"])
  cpSync(file, `dist/${file}`);
execFileSync(
  process.platform === "win32" ? "npm.cmd" : "npm",
  [
    "ci",
    "--prefix",
    "dist",
    "--omit=dev",
    "--ignore-scripts",
    "--no-audit",
    "--no-fund",
  ],
  { stdio: "inherit" },
);
