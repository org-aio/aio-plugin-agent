import {
  createAgentSession,
  ModelRuntime,
  SessionManager,
  SettingsManager,
} from "@earendil-works/pi-coding-agent";
import { InMemoryCredentialStore } from "@earendil-works/pi-ai";
import { streamSimple } from "@earendil-works/pi-ai/api/openai-completions";
import { createResources } from "./resources.mjs";

const usage = () => ({
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  totalTokens: 0,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
});

export async function runSession(request, bridge) {
  const settingsManager = SettingsManager.inMemory({
    compaction: { enabled: false },
    retry: { enabled: false, provider: { maxRetries: 0 } },
    enableAnalytics: false,
    enableInstallTelemetry: false,
    images: { blockImages: true },
    defaultProjectTrust: "never",
    packages: [],
  });
  const modelRuntime = await ModelRuntime.create({
    credentials: new InMemoryCredentialStore(),
    modelsPath: null,
    allowModelNetwork: false,
    refreshOnCreate: false,
  });
  modelRuntime.registerProvider("aio", {
    api: "openai-completions",
    baseUrl: "https://aio.invalid/v1",
    apiKey: "managed-by-aio",
    models: [
      {
        id: request.model,
        name: request.model,
        reasoning: false,
        input: ["text"],
        contextWindow: 65536,
        maxTokens: 16384,
        cost: usage().cost,
        compat: { supportsDeveloperRole: false, supportsStore: false },
      },
    ],
    streamSimple: (model, context, options) =>
      streamSimple(model, context, {
        ...options,
        fetch: bridge.fetch,
        apiKey: "managed-by-aio",
        maxRetries: 0,
        maxTokens: 16384,
      }),
  });
  const systemPrompt = request.messages
    .filter((message) => message.role === "system")
    .map((message) => message.content)
    .join("\n\n");
  const resourceLoader = await createResources(systemPrompt, bridge);
  const sessionManager = SessionManager.inMemory(process.cwd());
  const messages = request.messages.filter(
    (message) => message.role !== "system",
  );
  const prompt = messages.pop();
  if (prompt?.role !== "user" || typeof prompt.content !== "string")
    throw new Error("Invalid prompt");
  for (const message of messages) {
    if (message.role === "user")
      sessionManager.appendMessage({ ...message, timestamp: Date.now() });
    else if (message.role === "assistant")
      sessionManager.appendMessage({
        role: "assistant",
        content: [{ type: "text", text: message.content }],
        provider: "aio",
        model: request.model,
        api: "openai-completions",
        usage: usage(),
        stopReason: "stop",
        timestamp: Date.now(),
      });
    else throw new Error("Invalid history");
  }
  const { session, extensionsResult } = await createAgentSession({
    cwd: process.cwd(),
    agentDir: process.cwd(),
    settingsManager,
    resourceLoader,
    sessionManager,
    modelRuntime,
    model: modelRuntime.getModel("aio", request.model),
    thinkingLevel: "off",
    tools: request.tools,
  });
  try {
    if (extensionsResult.errors.length)
      throw new Error("Extension initialization failed");
    let tokens = 0;
    session.subscribe((event) => {
      if (
        event.type === "message_update" &&
        event.assistantMessageEvent.type === "text_delta"
      )
        bridge.send({ type: "text", text: event.assistantMessageEvent.delta });
      if (event.type === "message_end" && event.message.role === "assistant") {
        tokens += event.message.usage.totalTokens;
        bridge.send({ type: "usage", tokens });
      }
    });
    await session.prompt(prompt.content, { expandPromptTemplates: false });
    const answer = session.messages.at(-1);
    if (
      answer?.role !== "assistant" ||
      ["error", "aborted"].includes(answer.stopReason)
    )
      throw new Error("Agent response failed");
  } finally {
    session.dispose();
  }
}
