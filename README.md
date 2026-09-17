# 智能体 / AIO Agent

Dioxus Web + Rust Agent 执行器 + PostgreSQL。`frontend` 管理会话界面，`engine` 提供无界面、无 AIO 依赖的工具循环，`backend` 负责身份、加密、持久化与受控出站，`shared/rust` 共享传输模型。

Dioxus Web + Rust Agent executor + PostgreSQL. `frontend` manages the session UI, `engine` provides a headless tool loop without AIO dependencies, `backend` handles identity, encryption, persistence and controlled outbound access, and `shared/rust` holds the shared transport model.

执行器参考 [rust-ai-agent](https://github.com/solenovex/rust-ai-agent) 的消息—工具—结果循环设计，未复制整个 CLI 或引入进程级配置。模型连接与工具由调用方注入；运行时不依赖 Node、Pi SDK 或 JVM。库的 API、限制和验证见 [engine](engine/README.md)。

The executor follows the message-tool-result loop design of [rust-ai-agent](https://github.com/solenovex/rust-ai-agent) without copying the whole CLI or introducing process-level configuration. Model connections and tools are injected by the caller; the runtime does not depend on Node, the Pi SDK or a JVM. The library's API, limits and verification are in [engine](engine/README.md).

## 使用 / Usage

安装后在工作空间打开智能体，在“账户菜单 → 设置中心 → 插件设置”或市场详情打开配置，也可从对话中的设置进入。

After installation, open the Agent in the workspace and open its configuration from “账户菜单 → 设置中心 → 插件设置” (Account menu → Settings Center → Plugin Settings) or the marketplace details; the settings are also reachable from inside a conversation.

- 只填写模型服务 URL 和 API Key，名称自动生成；从兼容服务的 `/v1/models` 读取模型，支持同一个服务下的多个模型。
- 对话顶部可切换服务和模型，保留已有历史并从下一轮生效。排队和生成时暂不可切换，也可选择跟随空间模型。
- 网页搜索在插件设置中填写 Tavily API Key 并启用。密钥按当前租户和用户加密保存，界面只显示是否配置；留空保留，清除时禁用。工具调用使用 [Tavily Search API](https://docs.tavily.com/documentation/api-reference/endpoint/search)。
- 个人/团队记忆、来源引用、秘密查看和授权、待核实修订、会话搜索、知识图谱、停止生成、确认删除均在 Dioxus 界面完成。窄屏的历史和图谱可单独展开。
- 只有后端授权引用能打开记忆条目，模型伪造的引用不可点击；工具检索与来源访问仍校验当前空间权限。

- Only fill in the model service URL and API key; the name is generated automatically. Models are read from the compatible service's `/v1/models`, supporting multiple models under the same service.
- The service and model can be switched at the top of a conversation, keeping existing history and taking effect from the next turn. Switching is unavailable while queued or generating; you may also choose to follow the workspace model.
- Web search is enabled by filling in a Tavily API key in the plugin settings. The key is encrypted per tenant and user; the UI only shows whether it is configured. Leaving it empty keeps it; clearing it disables the feature. Tool calls use the [Tavily Search API](https://docs.tavily.com/documentation/api-reference/endpoint/search).
- Personal/team memory, source citations, secret viewing and authorization, pending revisions, conversation search, knowledge graph, stop generation and confirm-delete are all done in the Dioxus UI. History and graph can be expanded separately on narrow screens.
- Only backend-authorized citations can open memory entries; model-forged citations are not clickable. Tool retrieval and source access still validate current workspace permissions.

## 数据与执行边界 / Data & Execution Boundaries

会话和消息沿用现有 PostgreSQL 表。发送先保存加密收件与幂等 UUID，Memory 隔离秘密后再整理和生成。来源删除或撤权后，依赖内容不会显示或进入后续上下文。后台整理可恢复并由前台抢占；正在生成的回复每 250ms 持久化，浏览器每 650ms 读取快照，结束后停止轮询。

Sessions and messages reuse the existing PostgreSQL tables. Sending first persists encrypted receipts and idempotent UUIDs; Memory isolates secrets before organizing and generating. After a source is deleted or de-authorized, dependent content is neither displayed nor carried into later context. Background organization is resumable and preempted by the foreground; in-flight replies persist every 250ms, the browser reads snapshots every 650ms, and polling stops when done.

Rust 引擎直接处理兼容 `/responses` SSE，要求完整的 `response.completed` 事件；默认支持最多 8 轮工具调用、4 个前台并发和 120 秒总超时；启用桌面工具的生成使用 32 轮及至少 600 秒预算。工具包括授权的 `memory_search` 和可选 `web_search`，不开放 Shell 或本地文件执行。网页与记忆结果均作为不可信资料。模型凭据与工具密钥不写入日志、前端资产或 Git。

The Rust engine handles compatible `/responses` SSE directly and requires a complete `response.completed` event; it defaults to 8 tool-call rounds, 4 foreground concurrent turns and a 120-second total timeout; desktop-enabled turns use 32 rounds and a timeout of at least 600 seconds. Tools include the authorized `memory_search` and optional `web_search`; Shell or local file execution is not exposed. Web and memory results are both treated as untrusted material. Model credentials and tool keys are never written to logs, frontend assets or Git.

父插件 `aio-plugin-agent` 与记忆子插件 [aio-plugin-agent-memory](https://github.com/zjarlin/aio-plugin-agent-memory) 独立发布，以宿主桥调用而不共享数据库。后续子插件采用 `aio-plugin-agent-<功能名>` 命名。命令行笔记采集见 [CLI 指南](cli/README.md)。

The parent plugin `aio-plugin-agent` and the memory sub-plugin [aio-plugin-agent-memory](https://github.com/zjarlin/aio-plugin-agent-memory) are published independently and call each other through the host bridge without sharing a database. Future sub-plugins use the `aio-plugin-agent-<feature>` naming. Command-line note capture is documented in the [CLI 指南](cli/README.md) (CLI guide).

## 开发与验证 / Development & Verification

依赖 Rust nightly-2026-05-25、Dioxus CLI 0.7.9、Node 22+ 作者工具与独立 PostgreSQL；生产 Agent 仅运行 Rust ELF。Linux 交叉编译另需 cargo-zigbuild 和 Zig。

Requires Rust nightly-2026-05-25, Dioxus CLI 0.7.9, Node 22+ authoring tools and a dedicated PostgreSQL; the production Agent runs only a Rust ELF. Linux cross-compilation additionally needs cargo-zigbuild and Zig.

```sh
npm ci --ignore-scripts
npm run build
cargo build -p az-agent-server
export AIO_TEST_DATABASE_URL='postgres://developer@127.0.0.1:5432/agent_test'
node scripts/setup-dev.mjs
node scripts/preview-memory.mjs
```

完整记忆联调需相邻 Memory 仓库已构建 Component 和 release 开发运行器。测试库需撤销 public schema 的 PUBLIC 权限，详见该仓库开发宿主说明。开发配置保存在忽略提交的 `.local/runtime.json`（0600），重复初始化保留密钥并按校验和迁移。默认本地预览为 4192，设置页为 `/?page=settings`。

Full memory integration needs the neighboring Memory repository to have its Component and a release development runner built. The test database must revoke PUBLIC privileges on the public schema, per that repository's development host notes. Development configuration is stored in the git-ignored `.local/runtime.json` (0600); re-initialization keeps keys and migrates by checksum. The default local preview is 4192 and the settings page is `/?page=settings`.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
node scripts/test-service.mjs
npm run test:browser
```

服务回归使用真实数据库、Memory Component 和模拟 SSE 模型，覆盖工具、密钥隔离、租约/重试、停止/重启、撤权及修订。浏览器覆盖 1440×900 与 390×844。工具协议测试不代替真实 LLM 提取质量或付费 Tavily 联网验收。

Service regression uses a real database, the Memory Component and a simulated SSE model, covering tools, key isolation, leases/retries, stop/restart, de-authorization and revisions. Browser coverage includes 1440×900 and 390×844. Tool-protocol tests do not replace real LLM extraction quality or paid Tavily live-network acceptance.

## 自动交付 / Automated Delivery

默认分支推送后，平台按 `aio-delivery.toml` 在固定摘要的 Rust 构建镜像中生成 Dioxus 资产与 glibc 2.17 服务，发布 v2 包并更新仍启用该插件的租户。失败保留活动版本；停用、卸载不会被恢复。

After a default-branch push, the platform follows `aio-delivery.toml` to build Dioxus assets and a glibc 2.17 service in a checksum-pinned Rust build image, publishes a v2 package and updates tenants that still have the plugin enabled. On failure the active version is kept; disable and uninstall are never restored.

插件清单声明 `settings_page` 与第三方 `http_endpoints`，要求宿主至少 2026.9.18。容器禁网，数据库、Memory、模型和第三方请求经 Unix broker；完整授权与镜像说明见 [部署边界](deployment/README.md)。上线以交付记录和实际挂载版本为准。

The plugin manifest declares `settings_page` and third-party `http_endpoints`, requiring a host of at least 2026.9.18. The container has no network; database, Memory, model and third-party requests go through a Unix broker. Full authorization and image notes are in [部署边界](deployment/README.md) (deployment boundaries). Go-live is judged by delivery records and the actually mounted version.

### 已配对设备 / Paired Devices

网页对话支持请求“打开 Postman”。先在 AIO“我的设备”为已升级的 macOS worker 启用应用控制。Agent 用当前账号身份列出设备并下发固定的打开应用能力，凭客户端返回的进程 ID 确认完成；多台设备时先选择目标。等待超时只报告结果待确认，不重新执行。宿主需要授权 `AIO_PROCESS_WORKER_CAPABILITIES=desktop.open-app`。此能力不提供任意 shell、键盘或鼠标控制。

Web conversations support requesting “打开 Postman” (Open Postman). First enable app control for an upgraded macOS worker under AIO “我的设备” (My Devices). The Agent lists devices using the current account identity and dispatches the fixed open-app capability, confirming completion with the client-returned process ID; with multiple devices, pick a target first. A wait timeout only reports the result as pending confirmation, without re-executing. The host must authorize `AIO_PROCESS_WORKER_CAPABILITIES=desktop.open-app`. This capability provides no arbitrary shell, keyboard or mouse control.

自然语言请求完整交给 Responses 工具循环处理，包括“打开wps输入helloworld”等复合指令，不再按“打开”截取余下文本作为应用名。打开、观察、输入和回读分别调用已授权工具；仅自动选取唯一在线且已授权设备，多台设备时先询问目标。

Natural-language requests, including compound requests to open an app and enter text, pass intact to the Responses tool loop. The backend does not treat everything after “打开” as an application name. Opening, observing, typing and verification use authorized tools; a single online authorized device can be selected automatically, while multiple devices require a choice.

## 设备与 Skill / Devices & Skills

工作空间菜单为「智能体 → 对话 / 记忆图谱 / Skill 管理」。记忆图谱由可选 Memory 子插件提供。
配对使用 AIO 宿主的用户/租户体系，设备凭据可撤销；智能体通过能力协议调用 worker，
不直接访问客户端磁盘或其他插件的数据库。Skill 管理目前是 Agent 内的独立 feature/service，
不是一个需要再安装和登录的插件。宿主不会因卸载智能体而撤销其他插件使用的设备。

The workspace menu is 「智能体 → 对话 / 记忆图谱 / Skill 管理」 (Agent → Conversations / Memory Graph / Skill Management). The memory graph is provided by the optional Memory sub-plugin. Pairing uses the AIO host's user/tenant system and device credentials are revocable; the Agent calls workers through the capability protocol and never directly accesses client disks or other plugins' databases. Skill Management is currently a standalone feature/service inside the Agent, not a plugin that needs separate installation and login. The host does not revoke devices used by other plugins just because the Agent is uninstalled.

在已配对电脑运行 `aio-space skills-enable --path ~/.agents/skills`，即可每 30 秒双向同步；
`aio-space skills-sync` 立即运行，`aio-space skills-disable` 暂停并撤回同步能力。
正文和脚本、图片等资源一同同步；隐藏目录、Git、缓存和凭据文件不进入云端。
网页支持新建、编辑、删除文件和处理双端冲突。单边变化自动传播，双边变化需要选择版本；
本机覆盖/删除前留备份，云端保留加密历史。设备离线后恢复会重新比较，不按时间戳覆盖。
对话提供 `skill_list` / `skill_read` 按需读取当前用户技能，同步和读取本身都不会运行技能脚本。

Run `aio-space skills-enable --path ~/.agents/skills` on a paired computer to sync both ways every 30 seconds; `aio-space skills-sync` runs immediately, and `aio-space skills-disable` pauses and withdraws the sync capability. Bodies, scripts and assets such as images sync together; hidden directories, Git, caches and credential files never go to the cloud. The web UI supports creating, editing and deleting files and resolving two-sided conflicts. One-sided changes propagate automatically; two-sided changes require picking a version; local overwrites/deletes keep a backup first, and the cloud keeps encrypted history. When a device comes back online it re-compares instead of overwriting by timestamp. Conversations expose `skill_list` / `skill_read` to read the current user's skills on demand; neither syncing nor reading ever executes skill scripts.

参考设计：[DeepSeek Harness](https://www.deepseek.com/harness/) 将工具、技能、执行环境和 UI 作为可组合能力；
[字节 UI-TARS](https://github.com/bytedance/UI-TARS-desktop) 分开本地/远程电脑与浏览器操作器。
AIO 保留统一身份和远程设备通道，各业务能力与数据归插件所有。公开项目不代表豆包闭源客户端的内部实现。

Reference designs: [DeepSeek Harness](https://www.deepseek.com/harness/) treats tools, skills, execution environments and UI as composable capabilities; [ByteDance UI-TARS](https://github.com/bytedance/UI-TARS-desktop) separates local/remote computers from browser operators. AIO keeps a unified identity and remote device channel, while business capabilities and data belong to each plugin. Public projects do not represent the internal implementation of Doubao's closed-source client.

验证：`cargo test -p az-agent-server --lib`；`AIO_TEST_DATABASE_URL=... node scripts/test-skills.mjs`；
构建前端后运行 `AIO_TEST_DATABASE_URL=... node scripts/test-skills-browser.mjs`。
测试数据库必须是隔离的本机 PostgreSQL。浏览器验证包含桌面/移动端、新建、编辑、删除确认和溢出检查。

Verification: `cargo test -p az-agent-server --lib`; `AIO_TEST_DATABASE_URL=... node scripts/test-skills.mjs`; after building the frontend, run `AIO_TEST_DATABASE_URL=... node scripts/test-skills-browser.mjs`. The test database must be an isolated local PostgreSQL. Browser verification covers desktop/mobile, create, edit, delete confirmation and overflow checks.

桌面操作复用已配对 worker 与独立的本机授权。安装、观察凭据、截图协议及实际兼容性见 [桌面控制](docs/desktop-control.md)。

模型生成统一使用 Responses 协议，最低宿主版本为 2026.9.21。协议、检查点迁移和验证见 [执行器说明](engine/README.md)。
