# 智能体界面

Dioxus Web 前端，依赖共享 az-ui-components 的布局、主题、表单、Markdown 与 Dialog；业务不定义 CSS。通过 AIO Web SDK 获取已认证的数据，不接触服务密钥。

主入口 index.html 为会话工作空间；同一构建的 settings.html 标记为独立设置页，由宿主插件设置 Dialog 挂载。对话、模型选择、记忆来源/权限、图谱各自独立，传输模型来自 shared/rust。

构建：在仓库根运行 `dx build --package az-agent-frontend --platform web --release`，再运行 `node scripts/package-frontend.mjs`。完整浏览器验证见 scripts/README.md。

对话工作区采用共享 Codex 外观；入口、交互及验收边界见 [对话工作区](../docs/conversation-ui.md)。本地视觉验收运行 `npm run preview:ui`，自动验收运行 `npm run test:ui`，均使用明确标识的内存示例数据。
