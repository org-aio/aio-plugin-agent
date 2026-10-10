// 普通会话保留空间默认、服务默认和显式模型三种原有语义。
export function modelSelection(value) {
  if (!value) return { providerId: null, model: null };
  if (value.startsWith("auto:")) {
    return { providerId: value.slice(5), model: null };
  }
  const [providerId, model] = JSON.parse(value);
  return { providerId, model };
}

export function selectedModel(conversation) {
  if (!conversation?.providerId) return "";
  return conversation.model
    ? JSON.stringify([conversation.providerId, conversation.model])
    : `auto:${conversation.providerId}`;
}

export async function discoverModels(providers, discover, isCurrent) {
  const snapshot = providers.map((provider) => ({ ...provider }));
  const choices = new Array(snapshot.length);
  let next = 0;
  // 宿主桥最多保留 16 个请求，模型发现给会话和页面操作留出容量。
  async function loadNext() {
    while (next < snapshot.length && isCurrent()) {
      const index = next++;
      const provider = snapshot[index];
      try {
        const models = await discover(provider);
        choices[index] = [
          { id: `auto:${provider.id}`, label: `Auto · ${provider.label}` },
          ...models.map((model) => ({
            id: JSON.stringify([provider.id, model]),
            label: `${model} · ${provider.label}`,
          })),
        ];
      } catch {
        choices[index] = [
          {
            id: JSON.stringify([provider.id, provider.model]),
            label: provider.model,
          },
        ];
      }
    }
  }
  await Promise.all(
    Array.from({ length: Math.min(4, snapshot.length) }, () => loadNext()),
  );
  return choices.flat();
}
