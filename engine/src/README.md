# 执行与协议

model 定义增量和工具契约；execution 编排 Responses 与工具循环；stream 解析有界 SSE，并在 response.completed 后校验完整输出；checkpoint 只负责已有持久化执行状态的一次性升级。请求模板由宿主注入，执行器不接触全局凭据。详见上级 README。
