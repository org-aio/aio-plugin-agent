# Agent 执行器

参考 [软件工艺师的 rust-ai-agent](https://github.com/solenovex/rust-ai-agent) 的执行循环、工具接口与会话分离思路实现。此 crate 不依赖 AIO、Dioxus、数据库、环境变量或 Node；会话持久化和授权在调用方。没有复制参考项目的文件工具、MCP 子进程或全局 API Key 读取逻辑。

`run(request, model, messages, tools, output)` 接受已经授权的 reqwest 请求模板、Responses 输入项、工具实例及有界增量通道。模型在每次调用时注入，所以同一会话可以在下一轮更换模型。工具名称仅用于模型线协议，不作为应用插件的运行时注册身份。

流中断、非成功 HTTP、非法工具和上下文超额返回错误；取消直接丢弃执行 future。工具失败返回不含内部错误信息的模型可读结果。默认最多 8 轮，宿主可以通过 RunState.rounds_left 设置预算，每轮最多 8 个工具；不自动重试已经输出文本或执行工具的请求。

验证：`cargo test -p az-agent-engine`。

无参数且明确禁止额外字段的对象工具支持空 arguments；其他非法 JSON 只返回工具错误，模型可在 8 轮上限内修正，未通过解析时不调用工具。

执行器还支持 `resume(request, model, RunState, tools, output)`。工具返回 `InputRequired` 时输出 `Delta::Waiting { state, input }` 并结束当前调用。宿主负责加密持久化、授权及收集答案；`state.answer(value)` 将用户输入作为原工具结果注入，`input.retry` 为真时保留当前工具供重新校验。恢复继续剩余调用与轮数，不能从头重放整个 turn。参见 `src/input_tests.rs` 和 `../docs/device-orchestration.md`。

桌面图片通过可信 `Tool::take_images` 提取，编码数据从文本回执移除。本轮所有 tool 回执补齐后才追加包含 `input_image` 的观察消息，防止破坏 Responses 工具调用关联；暂停时图片随 RunState 保存，恢复不重放工具。只保留最新一批图片，单轮最多 2 张、每张 data URL 不超过 400 KB，上下文最多 1.5 MB。含桌面图片的请求在产生流输出前收到 HTTP 400 时，仅重试一次文字观察；保留工具回执、用户原始图片及模型配置，不重做已执行动作。该轮后续观察使用文字并明确禁止声称看图或猜测坐标，仍可通过辅助功能元素和文件校验完成建表；需要视觉判断的任务须选择支持图片的模型。401/403、无桌面图的错误、重试失败和流中断均不降级。


## Responses 协议

唯一生成协议为 `POST /responses`。请求使用 `input`、`stream: true`、`store: false` 和 `include: ["reasoning.encrypted_content"]`；工具声明使用顶层 `name`、`parameters`、`strict: false`，保留现有可选参数行为。主模型和鉴权仍由宿主注入。

`stream` 按事件类型读取文本，只有 `response.completed` 且状态为 completed 才接受结果；`[DONE]` 不能代替完成事件。失败、截断或取消不会执行尚未完成的工具。完成事件中的全部 output 项原样加入下一轮，包括 reasoning 的加密内容、消息 phase、函数 call_id；`function_call_output` 按 call_id 回传。这里的上下文保留覆盖单次执行的工具轮次及暂停恢复，新的用户消息仍由后端净化后的会话历史构建。

RunState 的 messages 字段现在保存 Responses 输入项。旧加密检查点在解密后一次性升级到 protocol_version=1，已执行的工具结果、待执行位置、图片和轮数配额均保留；未增加 Chat 出站或失败时切换协议。升级宿主至 2026.9.21 后才能安装此插件。

验证：`cargo test -p az-agent-engine` 覆盖逐字节 SSE、Unicode、推理项回传、工具参数、截图、暂停恢复、取消和失败边界；后端数据库测试使用隔离 schema 串行运行，避免两个实例争用同一租约。
