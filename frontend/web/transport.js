export const request = (method, path, body) =>
  window.aioPlugin.json(method, path, body);
export const memory = (method, path, body = null) =>
  request("POST", "/memory", { method, path, body });
export const fragment = async (section, data) =>
  (await request("POST", "/ui/render", { section, data })).html;

const versions = new WeakMap();
const messageCaches = new WeakMap();

// 每条消息单独渲染并缓存，流式更新只传输变化的消息，不累积整个会话请求体。
async function messagesFragment(element, data) {
  const previous = messageCaches.get(element) ?? new Map();
  const next = new Map();
  const parts = [
    [
      "header",
      "message-header",
      { earlier: data.earlier, empty: !data.messages.length },
    ],
    ...data.messages.map((message) => [message.id, "message", message]),
    [
      "footer",
      "message-footer",
      { pendingInput: data.pendingInput, approvals: data.approvals },
    ],
  ];
  const results = [];
  for (let index = 0; index < parts.length; index += 4) {
    results.push(
      ...(await Promise.all(
        parts.slice(index, index + 4).map(async ([id, section, value]) => {
          const key = `${section}:${id}`;
          const signature = JSON.stringify(value);
          const cached = previous.get(key);
          const html =
            cached?.signature === signature
              ? cached.html
              : await fragment(section, value);
          next.set(key, { signature, html });
          return html;
        }),
      )),
    );
  }
  messageCaches.set(element, next);
  return results.join("");
}

// 片段由 Topcoat 渲染和转义；过期响应不能覆盖已切换的会话。
export async function render(element, section, data) {
  if (!element) {
    return;
  }
  const version = (versions.get(element) ?? 0) + 1;
  versions.set(element, version);
  const html =
    section === "messages"
      ? await messagesFragment(element, data)
      : await fragment(section, data);
  if (versions.get(element) !== version || !element.isConnected) {
    return;
  }
  if (element.innerHTML === html) {
    return;
  }
  const follow =
    element.scrollHeight - element.scrollTop - element.clientHeight < 90;
  const previous = element.scrollTop;
  // 异步模型刷新不能关掉用户刚打开的菜单或展开的历史分组。
  const openMenus = [
    ...element.querySelectorAll(".dx-select[data-state=open]"),
  ].map((menu) => menu.querySelector("button").getAttribute("aria-label"));
  const groups = new Map(
    [...element.querySelectorAll(".dx-conversation__history-group")].map(
      (group) => [group.querySelector("summary").textContent, group.open],
    ),
  );
  const expanded = [...element.querySelectorAll("details[open]")].map(
    (item) => ({
      message: item.closest("[data-id]")?.dataset.id,
      task: item.dataset.task,
      title: item.querySelector("summary")?.textContent,
    }),
  );
  element.innerHTML = html;
  for (const item of element.querySelectorAll("details")) {
    if (
      expanded.some(
        (before) =>
          before.message === item.closest("[data-id]")?.dataset.id &&
          before.task === item.dataset.task &&
          (before.task ||
            before.title === item.querySelector("summary")?.textContent),
      )
    ) {
      item.open = true;
    }
  }
  for (const menu of element.querySelectorAll(".dx-select")) {
    if (
      !openMenus.includes(
        menu.querySelector("button").getAttribute("aria-label"),
      )
    ) {
      continue;
    }
    menu.dataset.state = "open";
    menu.querySelector("button").setAttribute("aria-expanded", "true");
    menu.querySelector("[role=listbox]").hidden = false;
  }
  for (const group of element.querySelectorAll(
    ".dx-conversation__history-group",
  )) {
    const open = groups.get(group.querySelector("summary").textContent);
    if (open !== undefined) {
      group.open = open;
    }
  }
  element.scrollTop = follow ? element.scrollHeight : previous;
}
export const cancelRender = (element) => {
  if (element) {
    versions.set(element, (versions.get(element) ?? 0) + 1);
  }
};
export function report(error) {
  const target =
    document.querySelector("#dialog-error") ?? document.querySelector("#error");
  if (target) {
    target.textContent = error?.message || String(error);
    target.hidden = false;
  }
}
