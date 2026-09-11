# Agent 契约

Rust 模型是 HTTP 数据契约，Kotlin 模型由 shared/contract.json 生成，避免手工维护两份字段定义。
模型不依赖 UI、HTTP 框架或数据库实现。子插件通过服务契约交流，不共享表和语言对象。
