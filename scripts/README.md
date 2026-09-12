# 作者工具

package-runtime.mjs 打包 Pi 执行模块并按锁文件安装生产依赖。runtime-tests.mjs 在实际 Rust/Pi/Memory 链路中验证原生工具循环、受控模型凭据、引用和图谱激活；模型模拟服务同时接受字符串与标准分段消息。

构建、契约生成、开发数据准备和浏览器验收都在安装前由作者执行。生产安装不得运行这些脚本。

`preview-memory.mjs` 同时启动 Agent 与相邻 Memory 开发宿主，生成仅本次运行有效的跨插件票据并选择空闲 loopback 端口。`test-service.mjs` 与 `test-browser.mjs` 使用相同的真实 PostgreSQL/Component/process 链路；模型端点由可检查 SSE 测试服务提供。`rehearse-memory.mjs` 只对本地验收数据库执行备份与新库恢复，不替代生产演练。

`AIO_MEMORY_BROWSER_ONLY=1 node scripts/test-service.mjs` 单独运行桌面/移动端对话与图谱流程；使用 `AIO_MEMORY_TEST_DIRECTORY` 隔离数据，`AIO_MEMORY_TEST_PORT` 和 `AIO_MEMORY_BROWSER_PORT` 指定空闲端口。截图与激活节点像素检查保存在 test-results。Compose beta 的原生弹窗关闭后存在语义树残留，凭据流程与图谱流程通过会话重载分别验收。
