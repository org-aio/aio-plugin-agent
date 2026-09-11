import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync, readFileSync } from "node:fs";
const schema = JSON.parse(
  execFileSync(
    "cargo",
    ["run", "--quiet", "-p", "az-agent-model", "--example", "contract"],
    { encoding: "utf8" },
  ),
);
function type(s) {
  if (s.$ref) return s.$ref.split("/").at(-1);
  if (s.anyOf) {
    const types = s.anyOf.filter((v) => v.type !== "null");
    if (types.length === 1) return `${type(types[0])}?`;
  }
  if (Array.isArray(s.type)) {
    const types = s.type.filter((v) => v !== "null");
    if (types.length === 1) return `${type({ ...s, type: types[0] })}?`;
  }
  if (s.type === "string") return "String";
  if (s.type === "boolean") return "Boolean";
  if (s.type === "integer") return "Long";
  if (s.type === "array") return `List<${type(s.items)}>`;
  throw new Error(`Unsupported schema: ${JSON.stringify(s)}`);
}
const models = Object.entries(schema.$defs)
  .map(([name, definition]) => {
    if (definition.type !== "object") throw new Error(`Not a record: ${name}`);
    const fields = Object.entries(definition.properties).map(
      ([field, value]) => {
        const t = type(value);
        return `    val ${field}: ${t}${t.endsWith("?") ? " = null" : ""}`;
      },
    );
    return `@Serializable\ndata class ${name}(\n${fields.join(",\n")}\n)`;
  })
  .join("\n\n");
const path = "shared/model/src/site/addzero/aio/agent/model/AgentModel.kt";
const code = `// 由 Rust JsonSchema 生成，修改模型后重新运行生成器。\npackage site.addzero.aio.agent.model\n\nimport kotlinx.serialization.Serializable\n\n${models}\n`;
if (process.argv.includes("--check")) {
  if (readFileSync(path, "utf8") !== code)
    throw new Error("Generated Kotlin contract is stale");
} else {
  mkdirSync("shared/model/src/site/addzero/aio/agent/model", {
    recursive: true,
  });
  writeFileSync(path, code);
  writeFileSync("shared/contract.json", JSON.stringify(schema, null, 2) + "\n");
}
