import {
  createEventBus,
  discoverAndLoadExtensions,
} from "@earendil-works/pi-coding-agent";
import { fileURLToPath } from "node:url";

export async function createResources(systemPrompt, bridge) {
  const eventBus = createEventBus();
  eventBus.on("aio:request", async ({ name, args, resolve, reject }) => {
    try {
      if (name === "system") resolve(systemPrompt);
      else if (name === "memory_search") resolve(await bridge.tool(name, args));
      else reject(new Error("Capability not authorized"));
    } catch (error) {
      reject(error);
    }
  });
  const root = fileURLToPath(new URL("./resources", import.meta.url));
  const extensions = await discoverAndLoadExtensions(
    [fileURLToPath(new URL("./extensions/memory.mjs", import.meta.url))],
    root,
    root,
    eventBus,
  );
  return {
    getExtensions: () => extensions,
    getSkills: () => ({ skills: [], diagnostics: [] }),
    getPrompts: () => ({ prompts: [], diagnostics: [] }),
    getThemes: () => ({ themes: [], diagnostics: [] }),
    getAgentsFiles: () => ({ agentsFiles: [] }),
    getSystemPrompt: () => systemPrompt,
    getSystemPromptSource: () => undefined,
    getAppendSystemPrompt: () => [],
    getAppendSystemPromptSources: () => [],
    extendResources() {
      throw new Error("Dynamic resources are not authorized");
    },
    async reload() {},
  };
}
