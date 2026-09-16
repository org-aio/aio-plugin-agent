# 作者工具

`model-browser.mjs` 验证手动修改 API 基址、加载远端模型列表和会话中切换模型，纳入桌面及移动端浏览器流程。服务测试同时覆盖列表鉴权、拒绝重定向、保留历史和下一轮使用新模型。

package-runtime.mjs 打包 Pi 执行模块并按锁文件安装生产依赖。runtime-tests.mjs 在实际 Rust/Pi/Memory 链路中验证原生工具循环、受控模型凭据、引用和图谱激活；模型模拟服务同时接受字符串与标准分段消息。

构建、契约生成、开发数据准备和浏览器验收都在安装前由作者执行。生产安装不得运行这些脚本。

`AIO_MEMORY_NOTES_ONLY=1 node scripts/test-memory.mjs` 通过 `memory-local-notes-tests.mjs` 验证 CLI 笔记采集、真实 Pi/Memory 队列、wiki 校验、密码隔离、重复文件可恢复清理和对话图谱召回；使用独立测试数据库与临时笔记，不读取个人资料。

`preview-memory.mjs` 同时启动 Agent 与相邻 Memory 开发宿主，生成仅本次运行有效的跨插件票据并选择空闲 loopback 端口。`test-service.mjs` 与 `test-browser.mjs` 使用相同的真实 PostgreSQL/Component/process 链路；模型端点由可检查 SSE 测试服务提供。`rehearse-memory.mjs` 只对本地验收数据库执行备份与新库恢复，不替代生产演练。

`AIO_MEMORY_BROWSER_ONLY=1 node scripts/test-service.mjs` 单独运行桌面/移动端对话与图谱流程；使用 `AIO_MEMORY_TEST_DIRECTORY` 隔离数据，`AIO_MEMORY_TEST_PORT` 和 `AIO_MEMORY_BROWSER_PORT` 指定空闲端口。截图与激活节点像素检查保存在 test-results。Compose beta 的原生弹窗关闭后存在语义树残留，凭据流程与图谱流程通过会话重载分别验收。
# v2 Process

build-process.sh 构建正式 Linux ELF，前端先运行 npm run build。Containerfile 的 runtime target 构建 Pi 运行镜像，清单固定其完整镜像摘要。整包通过 aio-platform 的 az-plugin-bundle package 示例生成并上传宿主 v2 发布接口。

宿主以固定 UID/GID `65532:65532` 运行 process。镜像内的 node 用户必须与之对应，并在系统账户记录中提供 `/app` 主目录；Rust 清空子进程环境后，Pi 仍会通过系统用户记录解析路径。不要依赖开发机的用户或主目录配置。构建镜像后执行 `node scripts/test-process-image.mjs sha256:<镜像摘要>`，在禁网、只读文件系统和生产 UID 下验证 Pi 流式会话及断线退出。测试使用合成模型响应，不调用真实模型，也不挂载宿主配置或凭据。
