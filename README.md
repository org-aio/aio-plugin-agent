# 智能体

`aio-plugin-agent`：Pi Agent 运行时 + Rust 持久化与鉴权服务 + 真实 Compose Web 界面 + PostgreSQL。同仓 `frontend/`、`backend/`、`runtime/`、`shared/`，Rust JsonSchema 生成 Kotlin 传输模型，不带 JVM。Pi 使用官方 `@earendil-works/pi-coding-agent` / `pi-ai` 0.85.1，界面不依赖 Pi 的 TUI。

运行时边界见 [Agent 运行时](runtime/README.md)。复用 Pi AgentSession、模型适配器、流处理和原生扩展工具循环；AIO 保留空间授权、秘密隔离、任务持久化和知识提交，通过无 UI 依赖的 JSON 进程协议连接。

## 子插件规范

- 父插件：`aio-plugin-agent`。
- 记忆子插件：[aio-plugin-agent-memory](https://github.com/zjarlin/aio-plugin-agent-memory)。
- 后续子插件一律 `aio-plugin-agent-<功能名>`，如 `aio-plugin-agent-tools`；不再使用独立顶级 `aio-plugin-<子功能>` 名称。
- 子插件 Kotlin 包为 `site.addzero.aio.agent.<功能名>`；Rust 包名保留 `az-` 前缀。仓库名表示来源和归属，不是 Dill 运行时类型身份。
- `family.json` 与检查脚本约束命名和归属。Agent 通过受信宿主桥调用 Memory，不直接访问子插件数据库；联合安装和回滚仍需正式宿主接入。

## 当前能力

- 模型配置、会话创建、历史查看、消息生成、停止生成、确认删除。
- 请求 OpenAI-compatible `/chat/completions`，Pi 解析真实 SSE，Rust 仅转发授权出站；正在生成时前端每 350ms 获取持久化快照，结束后停止轮询。现有桥不支持推送流，不宣称浏览器已直连 SSE。
- 输入、会话搜索、弹窗草稿是 Compose 本地状态。输入不会发送模型请求。
- 发送先在 PostgreSQL 事务中写入加密收件箱、请求指纹和“已收下”回复，随后由后台隔离秘密、检索资料并生成回复。普通消息表、历史上下文和引用仅使用净化内容。重复请求 UUID 不重复收件，不同内容复用同一 UUID 会被拒绝。
- Memory 管理空间、来源、秘密和整理租约；Agent 的独立后台任务执行模型请求并提交 wiki 修订。前台问答可抢占后台整理，暂停不消耗失败重试次数。每个空间需要显式绑定整理模型，未配置时继续收件。
- 会话未单独选择模型时，后续问答自动使用空间已授权的模型。先收件、后配置也能继续原对话，团队成员无需获取模型密钥。
- 个人与团队空间、成员角色、秘密单独授权、来源引用、待核实修订及受控秘密展示。空间管理员不会自动获得秘密和原文权限。来源删除或撤权后，历史中依赖该来源的内容不再显示，也不进入后续模型请求。
- 部分输出每 250ms 落库，进程异常退出后标记中断。净化及整理任务由持久状态恢复，保留原文和已提交知识；未完成的模型回复可继续对话。
- 模型凭据以 AES-256-GCM 加密，绑定租户、用户和配置 ID；前端只能看到是否已保存密钥。地址改变后不会沿用旧密钥。
- 服务仅能请求宿主允许的完整 API 基址，不跟随重定向，不读取代理环境变量。最多 4 个并发生成，120 秒总超时，输入和输出有界。
- 支持基于标题、别名、正文与图谱邻域的记忆检索，不依赖向量服务。Pi 可通过原生 `memory_search` 工具补充检索，结果重新校验当前空间权限，并写回来源引用和本轮图谱激活；初版不开放 Shell、文件读写或凭据调用外部服务。
- 聊天默认同时显示知识图谱：桌面并排、窄屏上下排列，可收起、缩放、暂停和切换节点列表。当前轮次命中与邻域高亮，历史轮次可重新选中；激活 ID 持久化且与累积引用分开。
- Memory 的纯 Kotlin 分类规则优先处理明确保存、查找和凭据定位。查找返回净化摘录和来源，不调用模型，也不生成 wiki 任务；明确保存仅后台整理可能使用模型。分析、复合和不确定请求回退模型，检索上下文上限为 6000 字符。回复中的 0 tokens 仅表示该次前台回复未调用模型。

接口参照 [Chat Completions 官方契约](https://developers.openai.com/api/reference/resources/chat)。自定义兼容服务须支持分段消息、`stream`、有效的 `finish_reason` 与 SSE `data: [DONE]`；模型 ID 由用户配置，没有写死默认模型。Pi SDK 参照 [官方 SDK 文档](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/sdk.md)。

## 本地运行

依赖：Node.js 22.19+、固定 Rust nightly 2026-05-25（Dill 要求）、Kotlin wrapper 0.12.0-dev-4233、独立开发 PostgreSQL。前端编译器 2.4.10 / Compose 1.12.0-beta03，资源包含本地中文字体，不用公网 CDN。

```sh
npm ci --ignore-scripts
AIO_GRAPH_SOURCE=../../kmp-aio/lib/compose/az-compose npm run build
cargo build --locked -p az-agent-server
export AIO_TEST_DATABASE_URL='postgres://developer@127.0.0.1:55432/agent_dev'
node scripts/setup-dev.mjs
npm run preview
```

默认预览 `http://127.0.0.1:4192/`，Rust 后端监听 loopback 4193。运行完整记忆链路使用 `node scripts/preview-memory.mjs`，要求相邻 Memory 仓库已构建 frontend、backend 和 release 开发运行器。开发身份固定为 `preview/developer`，不是公网登录实现。预览票据只在父页面，iframe 无法读取 Cookie 和父 DOM，宿主丢弃前端传入的租户/用户请求头。

`setup-dev.mjs` 创建独立 schema 和最小权限数据角色，在忽略提交的 `.local/runtime.json` 中以 0600 保存主密钥及连接配置；重复执行按校验和应用新迁移并保留原配置。备份此文件和数据库，丢失主密钥将无法解密模型凭据。默认仅允许 `https://api.openai.com/v1`。配置模型并将其绑定到空间后，已有待整理资料会自动进入后台模型队列。

其他服务由管理员配置 `allowedEndpoints`；本地兼容服务还需 `allowLoopback: true`。密钥不能写进 Git、前端资源或日志。生产应由宿主秘密管理器注入，而不是复制开发配置。

## 验证

图谱与 Memory 锁定同一个 az-compose 提交。上游是私有仓库，可通过 `AIO_GRAPH_SOURCE` 指定有读取权限的本地 Git 仓库，省略时构建会按锁定提交拉取。

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm run test:runtime
node scripts/check-family.mjs ../aio-plugin-agent-memory
node scripts/test-service.mjs
npm run test:browser
```

服务测试启动真实 Kotlin Memory Component、Agent process、独立 PostgreSQL schema 和可检查请求的 SSE 测试端点；覆盖秘密不泄露、租约回收、退避与失败重试、强制退出恢复、跨空间拒绝、独立授权、补充说明、wiki/别名/关系、人工冲突和回退。浏览器另起 4298/4299，验证桌面与移动端真实 Compose 对话和秘密查看/复制。`AIO_MEMORY_TEST_DIRECTORY` 可选择新的验收数据目录。**未配置真实模型凭据，协议测试不代表真实 LLM 提取质量验收。**

本地数据库副本演练运行 `node scripts/rehearse-memory.mjs`；只接受本机 PostgreSQL，在新数据库恢复备份和测试密钥，验证 Agent 数据角色、模型凭据、Memory 秘密可解密以及后台身份无法查看秘密。用 `AIO_MEMORY_TEST_DIRECTORY` 指向此前服务测试的目录；报告保存在忽略提交的 `build/`。这不是生产数据库演练。

## 公网状态

尚未部署公网。平台运行库已增加版本化加密、持久数据库绑定和 v2 Component 持久激活，但现役产品尚未接入该发布链，process 监督器仍缺受控数据库、跨插件与模型出站授权。部署门槛见 [部署边界](deployment/README.md)。开发桥不等于正式跨插件 broker，`family.json` 不等于联合生命周期。
