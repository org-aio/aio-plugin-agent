# 智能体界面

固定 Topcoat 0.6.2 的 Rust HTML 片段与浏览器原生 ES modules。`src/main.rs` 导出对话、设置与 Skill 管理三个入口；`src/lib.rs` 供后端认证后的 `/ui/render` 和本地夹具共用。所有动态文本经 Topcoat 转义，Markdown 另行净化。

共享外观来自 `frontend/style-source.json` 固定的 dioxus-admin-workbench 提交，由打包脚本校验 Git revision 后复制；不加载 Dioxus/Wasm，也不引入业务 CSS。浏览器通过正式 AIO SDK 使用当前身份，不接触服务密钥。

```sh
cargo run --locked -p az-agent-frontend -- dist/frontend
node scripts/package-frontend.mjs
npm run test:ui
npm run test:processing
npm run test:cloud
```

页面与协议边界见 [对话工作区](../docs/conversation-ui.md) 和 [云电脑](../docs/cloud-codex.md)。
