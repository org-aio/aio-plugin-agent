# 智能体服务

常驻 Rust process 负责 AIO 鉴权、加密、取消、并发配额和 PostgreSQL 持久化。模型协议与工具循环调用无界面 engine 库，不启动 Node/JVM 子进程。

conversation 管理会话、记忆授权与后台整理；web_search 管理用户独立的加密搜索配置与 Tavily 工具。前端仅获得 hasSecret，主密钥由宿主派生；生产进程无网络，模型/第三方 HTTP/跨插件调用均经 Unix broker。
