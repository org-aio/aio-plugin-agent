# 部署边界

`aio-plugin.toml` 声明原生 v2 process、独立设置页和权限。宿主 >=2026.9.21，受控构建使用 `scripts/build.sh --process` 生成 Dioxus 前端、设置入口及 glibc 2.17 Linux ELF。运行时固定 `Containerfile` 的 runtime 镜像摘要；镜像只包含 Rust ELF 所需系统动态库和证书，不包含 Node、Pi、JVM、Shell 或构建工具。

容器以 UID/GID 65532 运行、只读、禁网，通过 AIO_PLUGIN_CONFIG 读取宿主绑定并在 AIO_PLUGIN_SOCKET 提供服务。宿主独立数据库角色只有本插件 DML 权限，主密钥稳定派生，升级沿用原 schema；卸载不删除数据。激活先停止旧实例，不能宣称无缝并行滚动替换。

模型 API 基址需要同时声明在 `plugin.runtime.process.endpoints` 与管理员 `AIO_PROCESS_ENDPOINTS`，不允许重定向。公司服务填写 `https://company-ai.addzero.site/v1`；列表请求 `<基址>/models`，生成请求 `<基址>/responses`。地址改变时不复用旧 Key。

Tavily 使用 `https://api.tavily.com/search`，同时加入清单 http_endpoints 与宿主 AIO_PROCESS_HTTP_ENDPOINTS。插件用户在设置页填写 Key 后，经 `/egress/http` 执行固定 JSON POST；不能扩展为任意 URL、HTTP 方法或转发头。关闭搜索不会向第三方发请求。

Memory 根据 Git 来源定位同租户已启用的子插件。后台 service 身份不能查看秘密；交互式揭示需要仍有效的入站上下文。模型与工具仅接触净化消息和已授权资料。

独立本机预览使用 AIO_AGENT_DATABASE_URL、AIO_AGENT_MASTER_KEY、AIO_AGENT_INGRESS_TOKEN、AIO_AGENT_ENDPOINTS 与 Memory 开发桥变量。生产只读取宿主私有配置，不复制开发身份和密钥；不得将连接、Key 或票据写入 Git 和页面。

升级前备份插件数据库与宿主密钥，再核对发布源码 SHA、包摘要、租户活动版本和实际浏览器。数据库或 keyring 之一丢失都可能导致秘密无法恢复。
