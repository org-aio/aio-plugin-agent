# 对话工作区

从 `mod.rs::ConversationPage` 阅读：侧栏和消息流读取 `AgentState`，`composer.rs` 把输入、模型与执行设备组合为同一输入区；`message.rs` 负责 Markdown、引用和授权复制；`environment.rs` 显示当前会话环境；`router.rs` 明确呈现尚未接入的路由能力。模型目录由页面上的 `models::use_model_catalog` 持有，快捷条和下拉框共享。请求、轮询、取消与持久化继续走 `state` / `transport`。

共享 `az-ui-components` 通过 `data-appearance="codex"` 提供布局与主题；业务目录不包含 CSS。模型与设备列表向上展开，移动端侧栏使用遮罩，中文输入法选字不会提交。新建、设置和删除仍使用共享 Dialog。

在仓库根构建并打包后，`npm run preview:ui` 可查看真实 Wasm 的独立验收页面（内存示例数据，不连接真实账户）；`npm run test:ui` 验证布局和交互。生产接入仍使用 `npm run preview` 与正式宿主桥。
