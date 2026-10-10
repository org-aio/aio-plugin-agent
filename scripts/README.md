# 构建与验证

`build.sh` 构建 Topcoat 前端和 Rust 服务，`--process` 交叉编译正式 Linux/glibc 2.17 包；`package-frontend.mjs` 创建会话与插件设置两个入口。

`test-service.mjs` 启动原生 Memory 服务、Agent、独立 PostgreSQL schema 与可检查的 SSE 模型服务，验证隔离、队列、工具、重启和修订。先构建相邻 Memory 仓库的原生 `az-memory-server`，并设置指向一次性数据库的 AIO_TEST_DATABASE_URL。

`test-browser.mjs` 在上述链路上验证 Topcoat 桌面和移动端：URL/Key、动态模型、会话切换、可信引用、来源秘密、图谱与独立设置页。仅浏览器可用 `AIO_MEMORY_BROWSER_ONLY=1 node scripts/test-service.mjs`，数据目录用 AIO_MEMORY_TEST_DIRECTORY 隔离，截图写入忽略提交的 test-results。

`preview-memory.mjs` 复用 `memory-fixture.mjs` 启动原生 Memory、绑定 loopback 的凭据授权桥和 Agent 页面；桥仅用于本地联调，不提供独立 Memory 前端。`rehearse-memory.mjs` 只演练本地副本恢复。`check-family.mjs` 和 `check-sdk.mjs` 校验子插件归属与 SDK 来源。生产安装不执行作者脚本，不包含 Node 运行时。

`preview-conversation-ui.mjs` / `npm run preview:ui` 启动仅本地的内存夹具，使用真实打包页面 和正式 SDK 桥；不需要数据库或真实模型。`npm run test:ui` 验证 2009×1184 参考尺寸与桌面/手机、浅/深色、模型快捷条、路由能力状态、分组导航、环境信息、滚动边界、模型与设备选择、输入法、发送/停止、复制、新建和删除确认。截图与测量位于 `test-results/conversation-ui`。使用系统 Chrome 时设置 `AIO_UI_BROWSER_CHANNEL=chrome`；具体边界见 `docs/conversation-ui.md`。
