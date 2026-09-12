export default function memoryExtension(pi) {
  const invoke = (name, args) =>
    new Promise((resolve, reject) => {
      pi.events.emit("aio:request", { name, args, resolve, reject });
    });
  pi.on("before_agent_start", async () => ({
    systemPrompt: await invoke("system", {}),
  }));
  pi.registerTool({
    name: "memory_search",
    label: "检索记忆",
    description:
      "Search the current authorized memory space when more evidence is needed. Returns sanitized excerpts and source references, never secret values.",
    parameters: {
      type: "object",
      properties: { query: { type: "string", minLength: 1, maxLength: 180 } },
      required: ["query"],
      additionalProperties: false,
    },
    async execute(_id, args) {
      const result = await invoke("memory_search", args);
      return {
        content: [{ type: "text", text: JSON.stringify(result) }],
        details: {},
      };
    },
  });
}
