# 部署边界

模型地址校验复用 shared 协议库的 `process::model_endpoint`。正式 broker 允许已声明且获宿主批准的 HTTPS 或固定私网 IP 的 HTTP；例如本部署 Codex 网关 `http://192.168.31.252:18080/v1`，必须同时加入 `AIO_PROCESS_ENDPOINTS`。HTTP 域名、公网、回环和链路本地地址均不能通过正式出站。独立开发服务仍只接受 HTTPS 或显式开启的本机回环。

AIO 正式接入使用根目录 aio-plugin.toml 的 v2 process 包。宿主需具备固定镜像授权、专属 PostgreSQL 数据角色、持久派生密钥、交互上下文和 Unix broker；旧的零能力 process 入口不能安装本包。

正式宿主按租户启动无网络容器，通过只读挂载传入 AIO_PLUGIN_CONFIG，Agent 在 AIO_PLUGIN_SOCKET 提供服务。数据库只通过专属 Unix 通道连接；模型只能经宿主转发到清单和管理员共同授权的基址。Memory 按来源地址解析到同租户已启用的子插件，交互秘密访问需要仍有效的入站上下文，后台使用 service 身份。

先用 npm run build 生成 Compose 前端，再运行 sh scripts/build-process.sh 构建 Linux x86_64 ELF，要求 cargo-zigbuild、Zig 和 x86_64-unknown-linux-gnu target。Containerfile 的 runtime target 只包含固定版本 Node 和 Pi，不包含宿主密钥；运行时镜像摘要记录在清单中，ELF、前端和迁移由整包摘要绑定。本机默认构建当前架构，不能把 macOS 产物安装到 Linux。

构建产物包括 agent-server、runtime 和生产 node_modules；直接运行产物时设置 AIO_AGENT_RUNTIME_DIR 为 dist/runtime 的绝对路径，使用 Node 22.19+。容器固定 Node 22.23.1 并以非 root 用户启动，在 Linux 层重新安装生产依赖，避免复用开发机平台相关依赖。Node 子进程不继承服务凭据，模型 HTTP 字节通过父进程转发。

容器不内置 JVM；外部注入环境：AIO_AGENT_DATABASE_URL、AIO_AGENT_MASTER_KEY（32 字节 Base64）、AIO_AGENT_INGRESS_TOKEN（至少 32 字节）、AIO_AGENT_ENDPOINTS（逗号分隔的完整兼容 API 基址）。只从可信代理转发 x-aio-tenant-id、x-aio-user-id、x-aio-token；入口端口不得绕过代理公开。

上述 AIO_AGENT_* 环境变量仅用于独立服务和本地开发；Memory 开发桥另需 AIO_AGENT_MEMORY_URL 和 AIO_AGENT_MEMORY_TOKEN。正式 process 只读取宿主配置文件，不继承开发主密钥和 preview/developer 身份。

生产数据库由受控迁移器部署 backend/migrations 后只授予数据角色 DML 和序列权限。卸载不删除 schema。一次仅允许一个实例拥有同一 schema 的生成任务，切换必须先停止并排空旧实例，不能宣称此 process 示例已经支持无缝在线替换。
