# 智能体

Dioxus Web + Rust Agent 执行器 + PostgreSQL。`frontend` 管理会话界面，`engine` 提供无界面、无 AIO 依赖的工具循环，`backend` 负责身份、加密、持久化与受控出站，`shared/rust` 共享传输模型。

执行器参考 [rust-ai-agent](https://github.com/solenovex/rust-ai-agent) 的消息—工具—结果循环设计，未复制整个 CLI 或引入进程级配置。模型连接与工具由调用方注入；运行时不依赖 Node、Pi SDK 或 JVM。库的 API、限制和验证见 [engine](engine/README.md)。

## 使用

安装后在工作空间打开智能体，在“账户菜单 → 设置中心 → 插件设置”或市场详情打开配置，也可从对话中的设置进入。

- 只填写模型服务 URL 和 API Key，名称自动生成；从兼容服务的 `/v1/models` 读取模型，支持同一个服务下的多个模型。
- 对话顶部可切换服务和模型，保留已有历史并从下一轮生效。排队和生成时暂不可切换，也可选择跟随空间模型。
- 网页搜索在插件设置中填写 Tavily API Key 并启用。密钥按当前租户和用户加密保存，界面只显示是否配置；留空保留，清除时禁用。工具调用使用 [Tavily Search API](https://docs.tavily.com/documentation/api-reference/endpoint/search)。
- 个人/团队记忆、来源引用、秘密查看和授权、待核实修订、会话搜索、知识图谱、停止生成、确认删除均在 Dioxus 界面完成。窄屏的历史和图谱可单独展开。
- 只有后端授权引用能打开记忆条目，模型伪造的引用不可点击；工具检索与来源访问仍校验当前空间权限。

## 数据与执行边界

会话和消息沿用现有 PostgreSQL 表。发送先保存加密收件与幂等 UUID，Memory 隔离秘密后再整理和生成。来源删除或撤权后，依赖内容不会显示或进入后续上下文。后台整理可恢复并由前台抢占；正在生成的回复每 250ms 持久化，浏览器每 650ms 读取快照，结束后停止轮询。

Rust 引擎直接处理兼容 `/chat/completions` SSE，要求有效结束标记；支持最多 8 轮工具调用、4 个前台并发和 120 秒总超时。工具包括授权的 `memory_search` 和可选 `web_search`，不开放 Shell 或本地文件执行。网页与记忆结果均作为不可信资料。模型凭据与工具密钥不写入日志、前端资产或 Git。

父插件 `aio-plugin-agent` 与记忆子插件 [aio-plugin-agent-memory](https://github.com/zjarlin/aio-plugin-agent-memory) 独立发布，以宿主桥调用而不共享数据库。后续子插件采用 `aio-plugin-agent-<功能名>` 命名。命令行笔记采集见 [CLI 指南](cli/README.md)。

## 开发与验证

依赖 Rust nightly-2026-05-25、Dioxus CLI 0.7.9、Node 22+ 作者工具与独立 PostgreSQL；生产 Agent 仅运行 Rust ELF。Linux 交叉编译另需 cargo-zigbuild 和 Zig。

```sh
npm ci --ignore-scripts
npm run build
cargo build -p az-agent-server
export AIO_TEST_DATABASE_URL='postgres://developer@127.0.0.1:5432/agent_test'
node scripts/setup-dev.mjs
node scripts/preview-memory.mjs
```

完整记忆联调需相邻 Memory 仓库已构建 Component 和 release 开发运行器。测试库需撤销 public schema 的 PUBLIC 权限，详见该仓库开发宿主说明。开发配置保存在忽略提交的 `.local/runtime.json`（0600），重复初始化保留密钥并按校验和迁移。默认本地预览为 4192，设置页为 `/?page=settings`。

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
node scripts/test-service.mjs
npm run test:browser
```

服务回归使用真实数据库、Memory Component 和模拟 SSE 模型，覆盖工具、密钥隔离、租约/重试、停止/重启、撤权及修订。浏览器覆盖 1440×900 与 390×844。工具协议测试不代替真实 LLM 提取质量或付费 Tavily 联网验收。

## 自动交付

默认分支推送后，平台按 `aio-delivery.toml` 在固定摘要的 Rust 构建镜像中生成 Dioxus 资产与 glibc 2.17 服务，发布 v2 包并更新仍启用该插件的租户。失败保留活动版本；停用、卸载不会被恢复。

插件清单声明 `settings_page` 与第三方 `http_endpoints`，要求宿主至少 2026.9.18。容器禁网，数据库、Memory、模型和第三方请求经 Unix broker；完整授权与镜像说明见 [部署边界](deployment/README.md)。上线以交付记录和实际挂载版本为准。

### 已配对设备

网页对话支持请求“打开 Postman”。先在 AIO“我的设备”为已升级的 macOS worker 启用应用控制。Agent 用当前账号身份列出设备并下发固定的打开应用能力，凭客户端返回的进程 ID 确认完成；多台设备时先选择目标。等待超时只报告结果待确认，不重新执行。宿主需要授权 `AIO_PROCESS_WORKER_CAPABILITIES=desktop.open-app`。此能力不提供任意 shell、键盘或鼠标控制。
