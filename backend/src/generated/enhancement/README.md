# 运行时增强的模型插件

Agent 可选提供 MCP Streamable HTTP 工具 `enhancement_source`。它复用 `az-agent-engine` 的 Responses 执行、Agent 的加密模型配置、受控出站、并发配额与 token 上报。平台把该工具导入 `/v1/model/enhancement/source` 后，Rhai 和 TypeScript 的自举策略都可以使用它；平台内核不依赖模型 SDK，也不保存供应商密钥。

阅读顺序：`configuration.rs` 绑定部署身份，`controller.rs` 适配 MCP，`service_impl.rs` 重新读取模型授权并消费有界输出。`util.rs` 保存源码生成约束。生成不创建对话、不挂载设备或记忆工具、不调用模型工具，也不持有候选提交和版本激活权限。

## 部署绑定

先通过现有 Agent 设置保存供应商和模型。模型配置必须属于所选租户中的指定用户。该用户是部署方明确选定的模型额度所有者；MCP 不接受请求方传入的用户、租户、模型或地址。共享空间模型不在本入口的授权范围内。

在仓库外创建仅所有者可读的 JSON 文件（Unix 权限 `0600`），设置 `AIO_AGENT_ENHANCEMENT_BINDING` 为其路径：

```json
{
  "tenant_id": "your-tenant",
  "user_id": "model-owner",
  "provider_id": "00000000-0000-4000-8000-000000000001",
  "token": "REPLACE_WITH_A_RANDOM_SEPARATE_TOKEN",
  "public_origin": "https://agent.example.com"
}
```

`provider_id` 替换为已保存模型的真实 UUID；token 使用至少 32 字节的随机 URL 安全文本，必须与 `AIO_AGENT_INGRESS_TOKEN` / 宿主 ingress token 不同。未设置绑定文件时不提供 MCP 模型入口。宿主配置已绑定租户时，文件必须使用同一租户。模型接口只能使用现有 `RuntimeConfig.allowed_endpoints` / 宿主出站授权内的地址，不会读取全局 OpenAI 密钥或代理。

入口是 `POST /mcp/enhancements`，每个请求都需要 `Authorization: Bearer <模型生成 token>`。部署 origin 仅接受 HTTPS 或固定本机/私网 IP 的 HTTP；服务器校验 Host 和 Origin。使用 Unix socket 的托管 Sidecar 仍通过原有宿主 broker 出站；若要让另一宿主按 HTTP 导入其 MCP，需要由部署方提供受控的 HTTP 入口，当前平台不会自动把任意已安装组件变成模型能力。

## 导入平台能力

使用 MCP SDK 获取 `tools/list`，选中 `enhancement_source`。把它的 **完整 `inputSchema`、`outputSchema`** 分别复制到平台部署文件的 `capability.input_schema`、`capability.output_schema`；保留 `$schema` 和 `title` 等字段。平台会重新发现工具并逐项比对契约，不能用空 schema 代替。

在已有 `AIO_REMOTE_CAPABILITIES` 文件中添加一项：

```text
tenant_id: 与模型绑定相同的租户
capability.path: /v1/model/enhancement/source
capability.description: 生成 Rhai/TypeScript 增强源码
capability.effect: write
capability.input_schema: 上述 inputSchema
capability.output_schema: 上述 outputSchema
endpoint: https://agent.example.com/mcp/enhancements
credential_env: AIO_ENHANCEMENT_MODEL_TOKEN
transport: {"protocol":"mcp","tool":"enhancement_source","output":"structured"}
```

该文件整体仍为 `{"schema_version":1,"imports":[...]}`。宿主的 `AIO_ENHANCEMENT_MODEL_TOKEN` 对应模型绑定 token，不能使用供应商密钥。`write` 表示请求会消耗额度；适配器不自动重试。角色还需获得显式 `runtime:/v1/model/enhancement/source` 授权，管理权限不自动包含它。双语言策略和演进授权见平台 `docs/plugin/evolution/README.md`。

工具接收 `{requirement, target, available}`，仅返回 `{source}`。目标路径、语言、依赖和真实验收样例由调用方指定；模型只填写源码。输入输出最多 64 KiB，源码最多 16 KiB；最多执行一轮 Responses 请求，不挂载任何工具。总生成时间取 Agent 配置与 25 秒的较小值，给运行时的 30 秒增强链预留校验提交时间。取消、超时、关闭或输出超额会丢弃模型 Future 并释放共享配额，不在后台继续生成。

完成事件必须是 `response.completed`；提前断流、未完成回复、模型工具调用、非 JSON、额外字段或空源码均失败。生成结束再次读取模型所有权、地址、型号和解密凭据，生成期间删除或更改配置则拒绝返回。模型生成服务不判断原始运行时用户的角色；运行时负责真实会话、租户、显式能力及提交前撤权校验，部署方不得把这个服务凭据绑给其他租户。

成功完成后沿用现有 token 上报；中断可能没有完整 usage，当前不承诺精确计费或失败记录的持久审计。MCP 传输取消不能证明上游模型服务已经停止计费。模型返回结果仍须经过运行时编译、新旧样例重放和按代数激活；本适配器本身不证明生成质量。

## 验证

```sh
cargo test -p az-agent-server generated::enhancement -- --test-threads=1
AIO_AGENT_DEV_DIRECTORY=/tmp/agent-test node scripts/setup-dev.mjs
AIO_INPUT_TEST_CONFIG=/tmp/agent-test/runtime.json cargo test -p az-agent-server generated::enhancement -- --include-ignored --test-threads=1
```

数据库准备命令需要 `AIO_TEST_DATABASE_URL` 指向一次性开发库。集成测试使用真实 MCP SDK、PostgreSQL、加密供应商配置和 Responses 引擎，只有最终上游模型是受控 HTTP/SSE 替身；验证双语言、固定身份、跨租户拒绝、断流、超额、取消、超时、模型删除与关闭，不调用收费服务。真实模型质量、无人值守调度和完整失败审计仍需独立验收。
