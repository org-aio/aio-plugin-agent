# AIO Agent

`aio-plugin-agent`：Rust 模型服务 + 真实 Compose Web 界面 + PostgreSQL。同仓 `frontend/`、`backend/`、`shared/`，Rust JsonSchema 生成 Kotlin 传输模型。后端是 Rust 常驻 process，不带 JVM。

## 子插件规范

- 父插件：`aio-plugin-agent`。
- 记忆子插件：[aio-plugin-agent-memory](https://github.com/zjarlin/aio-plugin-agent-memory)。
- 后续子插件一律 `aio-plugin-agent-<功能名>`，如 `aio-plugin-agent-tools`；不再使用独立顶级 `aio-plugin-<子功能>` 名称。
- 子插件 Kotlin 包为 `site.addzero.aio.agent.<功能名>`；Rust 包名保留 `az-` 前缀。仓库名表示来源和归属，不是 Dill 运行时类型身份。
- `family.json` 与检查脚本约束命名和归属，**不是自动安装、跨插件授权或联合回滚的实现**。记忆数据不直接暴露给 Agent，尚未实现自动检索或自动写入。

## 当前能力

- 模型配置、会话创建、历史查看、消息生成、停止生成、确认删除。
- 请求 OpenAI-compatible `/chat/completions`，Rust 读取真实 SSE；正在生成时前端每 350ms 获取持久化快照，结束后停止轮询。现有桥不支持推送流，不宣称浏览器已直连 SSE。
- 输入、会话搜索、弹窗草稿是 Compose 本地状态。输入不会发送模型请求。
- 会话和回复保存到 PostgreSQL，部分输出每 250ms 落库；重启保留数据，未完成任务标记为中断。最终保存失败时保留已保存内容，数据库恢复后的读取会修复中断状态，不会永久显示生成中。请求 UUID 防止同一次发送被重复执行。
- 模型凭据以 AES-256-GCM 加密，绑定租户、用户和配置 ID；前端只能看到是否已保存密钥。地址改变后不会沿用旧密钥。
- 服务仅能请求宿主允许的完整 API 基址，不跟随重定向，不读取代理环境变量。最多 4 个并发生成，120 秒总超时，输入和输出有界。
- **当前是 Agent 的模型与会话核心，不包含自动工具执行、模型自主规划、memory 自动抽取或 RAG。** 不在没有授权的情况下执行模型返回的工具调用。

接口参照 [Chat Completions 官方契约](https://developers.openai.com/api/reference/resources/chat)。自定义兼容服务须支持 `stream` 与 SSE `data: [DONE]`；模型 ID 由用户配置，没有写死一个会发生变化的默认模型。

## 本地运行

依赖：Node.js 22+、固定 Rust nightly 2026-05-25（Dill 要求）、Kotlin wrapper 0.12.0-dev-4233、独立开发 PostgreSQL。前端编译器 2.4.10 / Compose 1.12.0-beta03，资源包含本地中文字体，不用公网 CDN。

```sh
npm ci --ignore-scripts
npm run build
cargo build --locked -p az-agent-server
export AIO_TEST_DATABASE_URL='postgres://developer@127.0.0.1:55432/agent_dev'
node scripts/setup-dev.mjs
npm run preview
```

默认预览 `http://127.0.0.1:4192/`，Rust 后端监听 loopback 4193。开发身份固定为 `preview/developer`，不是公网登录实现。预览票据只在父页面，iframe 无法读取 Cookie 和父 DOM，宿主丢弃前端传入的租户/用户请求头。

`setup-dev.mjs` 创建独立 schema 和最小权限数据角色，在忽略提交的 `.local/runtime.json` 中以 0600 保存主密钥及连接配置；重复执行保留原配置。备份此文件和数据库，丢失主密钥将无法解密模型凭据。默认只授权 `https://api.openai.com/v1`，不自动调用；在模型设置中填写模型 ID 和密钥后，只有点击发送才会发出请求。

其他服务由管理员配置 `allowedEndpoints`；本地兼容服务还需 `allowLoopback: true`。密钥不能写进 Git、前端资源或日志。生产应由宿主秘密管理器注入，而不是复制开发配置。

## 验证

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
node scripts/check-family.mjs ../aio-plugin-agent-memory
node scripts/test-service.mjs
npm run test:browser
```

服务测试使用本地 SSE 协议测试端点，覆盖持久化、逐段输出、取消、幂等、租户/用户隔离、失败、密钥加密和重启。服务与浏览器测试分别准备隔离数据 schema；浏览器另起 4194/4195，不污染预览会话。首次运行测试需要上述 AIO_TEST_DATABASE_URL。**未配置真实模型凭据，协议测试不代表真实 LLM 推理验收。**

## 公网状态

尚未部署公网。现役 process 监督器拒绝数据库和出站能力，尚缺安全密钥注入；这里没有伪装可安装的清单，也没有修改壳的业务依赖。部署门槛、静态 Linux 构建和镜像见 [部署边界](deployment/README.md)。`family.json` 中的记忆子插件关系尚未转成宿主自动组合，不能声称联合生命周期已经完成。
