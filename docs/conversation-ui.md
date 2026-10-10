# 对话工作区

前端采用 Topcoat 0.6.2。`frontend/src/views/shell.rs` 生成静态壳，`frontend/src/lib.rs` 渲染认证后的 HTML 片段；`frontend/web/app.js` 管理状态、URL 和轮询，`interactions.js` 接收用户操作，`dialogs.js` 处理设置、技能和记忆。后端原有持久化 API 继续使用。

## 外观与范围

沿用原 Agent 的对话、历史分组、模型快捷栏、设备/项目选择、环境面板、设置、Skill 管理和记忆图谱。共享样式来源由 `frontend/style-source.json` 固定提交，打包时校验；业务不复制桌面 React 运行时，也不增加私有 CSS。

此迁移对应现有 AIO Agent 功能与布局。原版 Desktop 资源桥属于另一入口；Topcoat 页面不等于完整的 Codex Desktop 或 Buddy 设置界面。普通对话仍使用现有 Rust 执行器；云电脑使用 [原生 Codex](cloud-codex.md)。未接入的 Buddy 配置保持明确不可用，不显示虚假的 token 配额或权限开关。

## 交互与状态

- 模型快捷条与选择器共用目录。默认值分别表示跟随空间、跟随服务和显式选择模型。普通对话排队、生成或等待回答时不改变模型；云电脑读取原生模型目录与当前会话模型。
- 历史按记忆空间分组并支持搜索、折叠；手机使用抽屉。每个窗口独立保存设备、项目和会话路由，刷新及浏览器前进后退可恢复。
- Enter 发送、Shift+Enter 换行，输入法合成期间不发送。草稿在等待回复和记忆整理时保留；发送请求尚未确认时禁止重复发送或新建。
- 消息区独立滚动；用户离开底部后不抢滚动。按消息缓存 Topcoat 渲染结果，流式更新只重新渲染变化消息。展开的执行详情和任务结果跨刷新保留。服务端保留 3 MiB 单次请求限制，超大单条消息的渲染错误会明确显示。
- Markdown 转义原始 HTML、限制 URL；只有后端返回的记忆引用能打开条目。来源、关联图谱、复制回复继续通过正式桥执行。
- 新建、设置、结构化回答和删除使用共享 Dialog。删除当前会话同时清除路由中的旧会话 ID；Codex 会话采用原生归档语义。
- 任务列表区分排队、执行、取消、失败和待核对结果，显示错误、JSON 回执与安全位图截图。成功收到回执不等同于真实任务目标已完成。

## 构建和验证

```sh
cargo run --locked -p az-agent-frontend -- dist/frontend
node scripts/package-frontend.mjs
npm run test:cloud
npm run test:ui
npm run test:processing
npm run preview:ui
```

可用 `AIO_UI_BROWSER_CHANNEL=chrome` 或 `AIO_UI_BROWSER_EXECUTABLE` 选择已安装浏览器。UI 夹具覆盖参考尺寸、1440×900 桌面、390px 手机、浅深主题、菜单、模型/设备/项目持久化、输入法、发送/停止、新建和删除。截图与测量写入忽略提交的 `test-results`。

`preview:ui` 使用正式 SDK 桥与内存夹具，不调用真实模型或访问账户数据。服务测试使用独立 PostgreSQL、Memory 和模拟模型。真实设备连接及任务执行另行验收，不能由夹具测试代替。
