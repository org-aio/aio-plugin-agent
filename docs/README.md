# 功能与验证边界

- [对话工作区](conversation-ui.md)：Dioxus 页面结构、共享 Codex 外观、菜单与消息交互，以及真实 Wasm 的视觉验收。
- [设备编排](device-orchestration.md)：设备选择、结构化提问与执行恢复。
- [桌面操作](desktop-control.md)：桌面 Worker 与执行回执。
- [蜂群执行](swarm-execution.md)：独立子任务、状态与取消边界。

构建与真实服务测试见 [scripts/README.md](../scripts/README.md)。所有截图与测试报告写入忽略提交的 `test-results`；报告不得包含数据库连接、主密钥、模型密钥或真实个人资料。界面夹具、服务协议测试与真实模型/设备链路分别说明验证范围，不能互相替代。
