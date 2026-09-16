# 构建与验证

`build.sh` 构建 Dioxus 前端和 Rust 服务，`--process` 交叉编译正式 Linux/glibc 2.17 包；`package-frontend.mjs` 创建会话与插件设置两个入口。

`test-service.mjs` 启动真实 Memory Component、Agent、独立 PostgreSQL schema 与可检查的 SSE 模型服务，验证隔离、队列、工具、重启和修订。先构建相邻 Memory 仓库的组件和 release 开发宿主，并设置指向一次性数据库的 AIO_TEST_DATABASE_URL。

`test-browser.mjs` 在上述链路上验证 Dioxus 桌面和移动端：URL/Key、动态模型、会话切换、可信引用、来源秘密、图谱与独立设置页。仅浏览器可用 `AIO_MEMORY_BROWSER_ONLY=1 node scripts/test-service.mjs`，数据目录用 AIO_MEMORY_TEST_DIRECTORY 隔离，截图写入忽略提交的 test-results。

`preview-memory.mjs` 启动本地真实双插件桥；`rehearse-memory.mjs` 只演练本地副本恢复。`check-family.mjs` 和 `check-sdk.mjs` 校验子插件归属与 SDK 来源。生产安装不执行作者脚本，不包含 Node 运行时。
