# 部署边界

Agent 需要 Rust process 的 TLS 出站、专属 PostgreSQL 数据角色、主密钥与受信入口票据。当前公网监督器明确拒绝带网络、数据库能力的 process 插件，且不提供票据/密钥注入，因此这里不提供伪装可安装的 aio-plugin.toml，也不绕过既有门禁。

当前提供可运行的同仓应用、开发宿主和静态 Linux 容器构建。完成宿主能力绑定后才能发布正式插件包。此边界同样限制与 aio-plugin-agent-memory 的自动调用和联合生命周期，不把 family.json 当作运行时安装机制。

Linux 构建：安装 x86_64-unknown-linux-musl 与 musl 工具链后，运行 `AIO_RUST_TARGET=x86_64-unknown-linux-musl npm run build`，再按 Containerfile 构建镜像。本机默认构建当前架构，不能把 macOS 产物安装到 Linux。

容器不内置 JVM；外部注入环境：AIO_AGENT_DATABASE_URL、AIO_AGENT_MASTER_KEY（32 字节 Base64）、AIO_AGENT_INGRESS_TOKEN（至少 32 字节）、AIO_AGENT_ENDPOINTS（逗号分隔的完整兼容 API 基址）。只从可信代理转发 x-aio-tenant-id、x-aio-user-id、x-aio-token；入口端口不得绕过代理公开。

生产数据库由受控迁移器部署 backend/migrations 后只授予数据角色 DML 和序列权限。卸载不删除 schema。一次仅允许一个实例拥有同一 schema 的生成任务，切换必须先停止并排空旧实例，不能宣称此 process 示例已经支持无缝在线替换。
