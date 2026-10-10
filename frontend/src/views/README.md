# 界面片段

`shell.rs` 组合对话、设置、Skill 管理入口。`conversation.rs` 提供历史、消息、模型和设备选择；`settings.rs` 组合共享 Dialog；`skills.rs`、`memory.rs`、`tasks.rs` 分别呈现技能、记忆和执行回执。`widgets.rs` 复用共享控件样式。

由 `lib.rs` 选择片段，输入为显示所需 JSON，输出为经转义的 HTML。Markdown 只允许安全 URL 和后端返回的记忆引用；设备截图限制为常见位图。
