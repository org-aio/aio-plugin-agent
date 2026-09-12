# 传输模型

runtime.rs 定义无 UI 依赖的 JSON 行协议，版本 1。Start 只承载净化消息、模型标识与授权工具名；Fetch 只声明受控模型请求，真实端点及密钥由宿主绑定。Response/Chunk/End 传输原始 HTTP 字节，Tool/ToolResult 传输结构化工具调用，Text/Usage/Done/Error 表示任务输出。运行时模型不进入 Compose DTO 生成。
