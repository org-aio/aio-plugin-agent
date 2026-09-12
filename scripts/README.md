# 作者工具

构建、契约生成、开发数据准备和浏览器验收都在安装前由作者执行。生产安装不得运行这些脚本。

`preview-memory.mjs` 同时启动 Agent 与相邻 Memory 开发宿主，生成仅本次运行有效的跨插件票据并选择空闲 loopback 端口。`test-service.mjs` 与 `test-browser.mjs` 使用相同的真实 PostgreSQL/Component/process 链路；模型端点由可检查 SSE 测试服务提供。`rehearse-memory.mjs` 只对本地验收数据库执行备份与新库恢复，不替代生产演练。
