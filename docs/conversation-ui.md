# 对话工作区

智能体使用 Dioxus Web 实现聊天工作区。`frontend/src/conversation/mod.rs` 是入口；`sidebar.rs` 负责会话导航，`message.rs` 渲染 Markdown 和记忆引用，`composer.rs` 组织消息输入、模型与设备选择。状态、请求与轮询分别保留在 `state.rs`、`transport.rs`、`main.rs`。

## 外观来源与所有权

布局对照桌面安装包 26.915.31945 / build 9922 的静态资源：46px 顶栏、14px 系统字体、768px 内容宽度、22px 输入框圆角，以及分离的消息与输入区。样式由 `dioxus-admin-workbench/crates/ui/components/src/conversation/codex.css` 持有，通过固定 Git revision 的 `az-ui-components` 提供。插件只选择 `data-appearance="codex"`，没有复制第三方 React 运行时或打包应用资源。

这一实现覆盖会话侧栏、空会话、消息流、输入区、菜单、深浅主题和窄屏布局。AIO 的模型服务、设备、记忆、图谱、蜂群和设置继续使用真实已有能力；没有为外观增加不可用的 Git、终端、附件或权限控件。AIO 品牌与业务文案保留。原桌面窗口未能通过工具直接截图，因此尚未完成原窗口的逐像素差异比较。

## 交互与状态

- 桌面侧栏可折叠；窄屏变为带遮罩的抽屉，选中会话、遮罩与 Escape 均可关闭。
- 模型与执行设备沿用共享 Select，列表向上展开；选择成功后以后端返回的 Conversation 为准。读取 Signal 后先释放借用，再更新状态，避免设备选择时发生运行时冲突。
- Enter 发送、Shift+Enter 换行；输入法合成过程中不会发送。生成或等待结构化回答时禁用普通输入，停止操作调用原取消接口。
- 助手回复可通过正式宿主桥复制，成功后显示勾选；拒绝授权或复制失败显示错误。Markdown 引用、来源资料和关联图谱入口继续工作。
- 消息区独立滚动；用户离开底部时不抢滚动，切换会话后重置为跟随新会话。侧栏列表独立滚动。
- 新建、设置与删除使用共享 Dialog，删除必须经确认；更多菜单只包含已实现的功能。

## 验证与本地查看

在仓库根执行：

```sh
dx build --package az-agent-frontend --platform web --release --locked
node scripts/package-frontend.mjs
npm run test:ui
npm run preview:ui
```

没有下载 Playwright Chromium 时，可设置 `AIO_UI_BROWSER_CHANNEL=chrome` 使用本机 Chrome。测试生成 1440×900 与 390×844 的截图、浅深主题、菜单、弹窗与几何测量，写入 `test-results/conversation-ui`；同时检查会话操作的宿主桥请求和控制台错误。

`preview:ui` 仅使用内存夹具与正式 SDK 桥，不读取账户配置、不调用真实模型、不持久化真实数据。它验证真实 Wasm 界面及业务接线，不能替代 PostgreSQL、设备 Worker、Memory 或真实模型的完整链路验证。真实服务预览继续使用 `npm run preview`。
