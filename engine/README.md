# Agent 执行器

参考 [软件工艺师的 rust-ai-agent](https://github.com/solenovex/rust-ai-agent) 的执行循环、工具接口与会话分离思路实现。此 crate 不依赖 AIO、Dioxus、数据库、环境变量或 Node；会话持久化和授权在调用方。没有复制参考项目的文件工具、MCP 子进程或全局 API Key 读取逻辑。

`run(request, model, messages, tools, output)` 接受已经授权的 reqwest 请求模板、OpenAI 兼容历史、工具实例及有界增量通道。模型在每次调用时注入，所以同一会话可以在下一轮更换模型。工具名称仅用于模型线协议，不作为应用插件的运行时注册身份。

流中断、非成功 HTTP、非法工具和上下文超额返回错误；取消直接丢弃执行 future。工具失败返回不含内部错误信息的模型可读结果。最多 8 轮，每轮最多 8 个工具；不自动重试已经输出文本或执行工具的请求。

验证：`cargo test -p az-agent-engine`。

无参数且明确禁止额外字段的对象工具支持空 arguments；其他非法 JSON 只返回工具错误，模型可在 8 轮上限内修正，未通过解析时不调用工具。
