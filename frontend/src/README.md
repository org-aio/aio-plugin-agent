# Topcoat 渲染入口

`main.rs` 导出静态页面骨架，`lib.rs` 将经过鉴权的显示状态路由到 `views`。例如 `RenderRequest { section: "messages".into(), data }` 渲染消息，渲染本身不查询数据库、不执行模型或设备操作。

浏览器行为在 `../web`；后端调用见 `backend/src/transport.rs`。原 Dioxus 状态与组件已由这些入口替代。
