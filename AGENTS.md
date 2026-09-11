# Agent 开发约定

- 父插件名为 aio-plugin-agent；子插件仓库统一 aio-plugin-agent-<功能名>，记忆为 aio-plugin-agent-memory。
- frontend、backend、shared 同仓发布；前端使用 Compose，后端使用 Rust，不把业务注入宿主源码。
- 子插件关系是发布组合关系，不冒充 Rust TypeId，也不直接访问子插件数据库。
- 服务端密钥不能回传浏览器；模型请求只允许管理员配置的来源，不能跟随重定向。
- PostgreSQL 保存会话和消息；业务按功能组织，目录有 README，注释用中文，单文件不超过 800 行。
- 新建、设置使用 Dialog；删除需要确认。输入和视图状态留在 Compose 本地。
- 验证后只提交并推送本次完成修改，不夹带其他仓库未完成重构。
