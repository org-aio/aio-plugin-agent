# Agent 运行时

复用 Pi 的无头 AgentSession、模型适配器和 ExtensionAPI。Compose 只依赖 AIO 传输模型；Pi 的 TUI 不参与产品界面。

入口 main.mjs 使用 shared/rust/src/runtime.rs 定义的版本化 JSON 行协议。AIO 发送净化历史，运行时发出模型出站及 memory_search 请求；真实模型密钥、租户身份和数据库绑定不进入此进程。每次任务使用内存会话、内存凭据和显式扩展，不扫描用户全局插件、技能或 AGENTS.md。PostgreSQL 仍是会话与任务的持久化源。

扩展使用 Pi 原生工厂函数，在 resources.mjs 的清单中显式注册，标准模块位于 extensions。工具权限由 Rust 桥再次校验，不能用扩展参数指定租户、空间或秘密展示接口。初版开放净化记忆检索，不开放文件系统及 Shell 工具。Pi 的自动摘要暂时关闭，避免摘要脱离来源权限与删除规则；AIO 继续提供有界、重新鉴权的历史。

每次任务的 Node 进程只允许读取运行时代码与依赖，禁止文件写入、子进程、原生插件及 worker。扩展必须纳入受审部署清单，这些限制不代表可以执行任意恶意第三方插件。AIO_AGENT_RUNTIME_DIR 指定绝对目录，默认当前工作目录下的 runtime；AIO_AGENT_NODE 指定 Node 可执行文件，默认 PATH 中的 node。模型和工具请求共用 16 次调用上限，失败重试仍由 AIO 持久队列负责。
