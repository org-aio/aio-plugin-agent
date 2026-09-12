# 部署边界

Agent 需要 Rust process 的 TLS 出站、专属 PostgreSQL 数据角色、主密钥、受信入口票据和受控 Node 子进程。当前公网监督器明确拒绝带网络、数据库能力的 process 插件，且不提供票据/密钥注入，因此这里不提供伪装可安装的 aio-plugin.toml，也不绕过既有门禁。

当前提供可运行的同仓应用、Agent 与 Memory 联合开发入口和 Linux 容器构建。开发入口通过带鉴权的 loopback broker 自动调用 Memory；正式宿主仍需接入跨插件调用和联合生命周期，不能把 family.json 当作运行时安装机制。

Linux 构建：安装 x86_64-unknown-linux-musl 与 musl 工具链后，运行 `AIO_RUST_TARGET=x86_64-unknown-linux-musl npm run build`，再按 Containerfile 构建镜像。本机默认构建当前架构，不能把 macOS 产物安装到 Linux。

构建产物包括 agent-server、runtime 和生产 node_modules；直接运行产物时设置 AIO_AGENT_RUNTIME_DIR 为 dist/runtime 的绝对路径，使用 Node 22.19+。容器固定 Node 22.23.1 并以非 root 用户启动，在 Linux 层重新安装生产依赖，避免复用开发机平台相关依赖。Node 子进程不继承服务凭据，模型 HTTP 字节通过父进程转发。

容器不内置 JVM；外部注入环境：AIO_AGENT_DATABASE_URL、AIO_AGENT_MASTER_KEY（32 字节 Base64）、AIO_AGENT_INGRESS_TOKEN（至少 32 字节）、AIO_AGENT_ENDPOINTS（逗号分隔的完整兼容 API 基址）。只从可信代理转发 x-aio-tenant-id、x-aio-user-id、x-aio-token；入口端口不得绕过代理公开。

Memory 调用还需注入 AIO_AGENT_MEMORY_URL 和 AIO_AGENT_MEMORY_TOKEN。broker 必须由宿主从已认证请求构造租户、用户和后台身份，绑定同一租户下已激活的 Memory 包；后台身份禁止原文和秘密展示。当前仓库的 loopback broker 仅供固定开发身份验收。正式模型出站需容器网络层与服务端允许列表共同限制，不能只授予任意公网访问。

生产数据库由受控迁移器部署 backend/migrations 后只授予数据角色 DML 和序列权限。卸载不删除 schema。一次仅允许一个实例拥有同一 schema 的生成任务，切换必须先停止并排空旧实例，不能宣称此 process 示例已经支持无缝在线替换。
