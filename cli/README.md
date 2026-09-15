# AIO 智能体命令行与本机笔记

`aio-agent` 是智能体插件自己的无界面入口。它通过正式宿主的登录会话与 v2 请求桥访问 Agent，再由 Agent 调用 Memory；和 Compose 对话使用同一份空间、来源、wiki 和知识图谱，不另建离线问答数据库。

## 安装与登录

需要 Node.js 22.19+。当前从仓库安装，尚未发布独立 npm 包：

```sh
git clone https://github.com/zjarlin/aio-plugin-agent.git
cd aio-plugin-agent/cli
npm link
aio-agent --help
```

已有仓库可直接在其 `cli/` 目录执行 `npm link`，或者用 `node cli/src/main.mjs`。宿主需已安装并启用 Agent、Memory v2 插件，当前账号具有所选空间权限。

登录密码通过标准输入读取，不使用命令行参数或环境变量。macOS zsh 示例：

```sh
read -rs 'aio_login_password?AIO 登录密码: '
printf '%s' "$aio_login_password" | aio-agent login --url https://aio.addzero.site --account zjarlin --password-stdin
unset aio_login_password
```

仅保存宿主会话到 `~/.config/aio-agent/session.json`，权限为 0600；不保存密码或模型密钥。不同宿主或用户用 `--profile` 指定不同文件。会话过期时重新登录；任务不会自动绕过登录或用户撤权。

## 调取记忆

```sh
aio-agent memory spaces
aio-agent memory models
aio-agent memory search "项目发布流程" --space 空间ID
aio-agent memory get 节点ID --space 空间ID
aio-agent memory source 来源ID --space 空间ID
aio-agent memory capture --file ./笔记.md --space 空间ID --request-id 自己的稳定请求ID
aio-agent memory export 来源ID --space 空间ID --output ./wiki
```

`search` 检索标题、别名、正文和图谱邻域，输出净化上下文及节点引用，不调用大模型。`capture` 只输出来源 ID 和处理状态；重试时复用请求 ID。其他程序可直接消费 JSON 输出。查询不提供解密秘密的命令，密码仍在 AIO 受控凭据区域查看。

在 AIO 智能体选择同一空间，输入「查找 项目发布流程」，即可召回 CLI 导入的资料，聊天图谱高亮命中节点。未完成 wiki 整理的净化来源也可检索；分析性问题沿用该空间授权的问答模型。

## 配置笔记整理

```sh
aio-agent notes init \
  --config "$HOME/.config/aio-agent/notes.json" \
  --root /Users/zjarlin/aio/note \
  --output /Users/zjarlin/aio/llm-wiki
aio-agent notes accounts
```

`--root` 可重复指定，例如加入 iCloud Drive 的某个明确笔记目录。Apple「备忘录」使用 `notes accounts` 返回的账号 ID 填入 `appleAccounts`，通过 Notes 脚本接口读取纯文本，不直接读取其数据库；原有格式、附件和原件保留。锁定、共享及“最近删除”中的笔记跳过。默认账号列表为空，避免把其他账户一并收集。

先在 AIO 模型设置中配置真实的 q3-4b 兼容接口，再将目标空间绑定到它。`memory models` 和 `memory spaces` 可以查看配置 ID，填入生成的 JSON：

```json
{
  "version": 1,
  "profile": "/Users/zjarlin/.config/aio-agent/session.json",
  "roots": ["/Users/zjarlin/aio/note"],
  "appleAccounts": [],
  "output": "/Users/zjarlin/aio/llm-wiki",
  "spaceId": "从 memory spaces 复制",
  "modelBinding": "从 memory models 复制配置 ID",
  "model": "复制该配置的真实 model 字段",
  "cleanup": "none",
  "batchSize": 20
}
```

示例中的 ID 和 model 必须替换。没有硬编码或猜测 `q3-4b` 对应的服务模型名。任务会同时核对当前登录身份、空间写入权限、空间绑定和模型 ID，不一致则停止。模型请求继续由 Agent/Pi 受控出站，不从 CLI 直接发给其他模型。

```sh
aio-agent notes status --config "$HOME/.config/aio-agent/notes.json"
aio-agent notes scan --config "$HOME/.config/aio-agent/notes.json"
aio-agent notes run --config "$HOME/.config/aio-agent/notes.json"
```

`status` 检查本地必要配置是否齐全，不表示登录会话或模型已在线；`scan` 只输出数量，不收件、不请求模型、不删除。`run` 每次最多推进 20 份资料，先提交新资料，再核对上一批产物。原文整篇先交给 Memory 加密保存与秘密隔离，后台持久队列调用指定模型提取 wiki 和关系。下一次运行完成导出与清理；不会在一个 CLI 进程内一直等模型。

定时器只需要周期性调用 `notes run`。用 Codex 定时任务时，让它执行命令并检查数量结果，不把私人笔记复制到 Codex 上下文，不让调度模型替代 q3-4b 进行整理。机器休眠、登录过期、模型配置缺失时保留原件，恢复后继续。

## 产物与清理

输出目录包含：

- `index.md`：wiki 入口；`entries/<来源ID>/<节点ID>.md`：有版本和来源链接的条目。
- `sources/<来源ID>.md`：Memory 的逐字净化来源，避免只剩模型摘要。
- `graph.json`：使用 AIO 节点 ID 的图谱快照；最新图谱仍以 AIO 为准。
- `manifests/`：来源摘要、节点版本及依据；`state.json`：收件断点；`last-run.json`：本轮数量。

文件权限为 0600，新目录为 0700。原始秘密只留在 Memory 加密原文及独立秘密存储，不写入 wiki。净化快照仍是个人资料，不应提交公开仓库；本地已导出的快照不会因服务端撤权自动擦除，AIO 在线检索每次仍重新鉴权。

清理策略必须明确配置：

| cleanup | 行为 |
| --- | --- |
| `none` | 默认：整理并导出，保留所有原件。 |
| `duplicates` | 已核对后，把同名、同内容且仍有原件的重复文件移入废纸篓。 |
| `archived` | 已核对后，把符合条件的已归档原文件移入废纸篓。 |

“核对”包括来源状态为 complete、wiki 有对应来源依据、保存净化全文、输出文件哈希、再次读取来源及节点版本、移动前后重新读取原件。它保证来源和可恢复副本，不等于证明模型每条总结都正确。模型不能自行把一份笔记判为垃圾并扩大清理范围。

清理使用 `~/.Trash/AIO 已整理/`，不清空废纸篓、不永久删除；`state.json` 记录原路径与废纸篓路径。含秘密、附件/本地资源链接、冲突、未完成任务或修改中的笔记均保留。跨磁盘移动失败时也保留原件。Apple 备忘录目前只读采集，原件保留。

扫描只处理 UTF-8 Markdown/TXT，忽略符号链接、隐藏目录、编辑器和依赖缓存；SVG、图片、PDF、DOCX 等不删除。15 分钟内修改的资料延后处理。单份含标题超过 100000 字节时，因统一隔离入口配额保留并计入失败，不在秘密隔离前切断凭据字段。空白文件也不会仅因为空而自动删除。

## 验证

```sh
cd cli
npm test
cd ..
AIO_MEMORY_NOTES_ONLY=1 node scripts/test-memory.mjs
```

真实链路测试要求已有 Agent debug 可执行文件、相邻 Memory release 开发宿主和全新的独立 PostgreSQL（`AIO_TEST_DATABASE_URL`），输出目录用 `AIO_MEMORY_TEST_DIRECTORY` 隔离。它验证真实收件、Pi 队列、来源与版本、重复文件清理、密码不出现在模型请求/产物、AIO 对话召回及图谱激活。模型是可审计 SSE 模拟端点，该测试不代表 q3-4b 的真实提取质量验收。
