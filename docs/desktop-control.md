# 通用桌面操作

Agent → AIO 宿主授权桥 → 已配对 aio-space worker → Open Computer Use MCP → 本机桌面。结果沿原任务通道返回；界面和模型都能收到截图。模型仍在 Agent 中运行，worker 负责操作系统输入与观察，不在每台设备另起模型。

## 开启与升级

1. 宿主先升级到支持 `desktop.control` 的版本；Agent 清单声明该能力并通过插件授权流程取得权限。
2. 本机安装包含桌面适配器的 aio-space，沿用现有配对。运行 `aio-space desktop-enable` 后再以 `aio-space worker --background` 更新后台入口。
3. 在 macOS 系统设置中授予实际 worker / OCU 运行程序辅助功能与屏幕录制权限。保持图形用户会话已登录、桌面可操作。OCU 自带 `ocu doctor` 可检查原生权限。
4. 选择支持工具调用的模型及会话执行设备。依赖截图判断的操作还需要模型支持图像；纯文本模型可通过辅助功能文字和文件校验完成建表。已有“打开应用”或工作区权限不会自动扩大为桌面控制权限。

`aio-space desktop-status` 查询本机开关；`aio-space desktop-disable` 先关闭本机接收，再撤销服务端能力并取消未完成桌面任务。网络失败应重试服务端撤销。该授权覆盖本机账号可以通过桌面完成的操作，包括前台鼠标和键盘输入；它不是应用沙箱，也不受工作区目录边界限制。

## 执行契约

- `desktop_control` 提供 list_apps、get_app_state、activate_app、click、type_text、press_key、scroll、drag、set_value、perform_secondary_action、create_spreadsheet、release、wait。底层 MCP 工具固定，不接受远程可执行文件或 shell 参数。
- 会话 UUID 由后端绑定。一台 worker 同时只允许一个会话使用桌面；空闲 120 秒关闭 MCP，会话完成可主动 release。其他人手动操作桌面仍会影响界面，操作前须重新观察。
- get_app_state 返回截图、元素索引及 observation；动作必须使用相同应用的最新凭据。凭据有效期 120 秒且在副作用前消费一次，失败、断网或取消后不能用旧凭据重放。
- 动作后自动回读界面。工具 `success` 只说明原生调用及回读成功；“已创建”“已输入”“已保存”必须由界面内容或文件证据验证。queued/running 使用原 task_id 等待，不重新派发。
- 任务沿用设备租约与取消。模型轮数上限 32，总时限至少 600 秒；超出预算返回实际未完成状态。截图和界面文字都是不可信资料，不替代用户指令。
- 截图压缩为 JPEG，保留原始像素尺寸，避免坐标错位；超过 270 KB 时省略并明确说明，不能猜测坐标。图片以真正的多模态消息传给模型，不把 base64 放进文本工具结果。含桌面图的请求返回 HTTP 400 时，只重试一次文字观察，保留工具回执，不重复执行动作；本轮后续不再发送桌面图，也不删除用户原始图片或更换供应商。无截图时不得声称看图或猜测坐标；确需视觉判断的操作仍需支持图片的模型。
- Agent 加密保存回执及截图；任务界面只返回最新一张图片，历史截图以文字说明代替。刷新仍能读取持久化结果。

## 已验证与限制（2026-09-17）

模拟 MCP 测试覆盖观察凭据、会话独占、失败后不重放、图片尺寸与权限。模拟 HTTP/SSE 验证多工具图片顺序、旧图释放和暂停恢复。隔离 PostgreSQL 覆盖设备授权、租户隔离、任务查询和撤权取消；真实 wasm 页面在 1440×900 与 390×844 显示图片、刷新保留回执且无横向溢出。

macOS 上 OCU 0.3.5 的辅助功能和录屏检查通过，能够列举 WPS、读取辅助功能树并返回 1280×746 截图。WPS 首页“新建”点击及快捷键仍未观察到可验证的页面变化，原生输入兼容性待解决。通过新增 create_spreadsheet 已完成实际验收：生成并保存“姓名/年龄、小明/18”的 XLSX，重新解析导出文件核对单元格及 SHA-256，再交给 WPS 打开；返回窗口标题“小明年龄表.xlsx”，实际截图中 A1:B2 内容一致。这证明建表、保存和打开成功，不代表鼠标点击或任意表格编辑均已验收。activate_app 复用现有 macOS 启动器；Windows/Linux 未进行端到端验收。

本改动不是生产上线证明。宿主、Agent 和 worker 必须配套升级并完成本机授权；仓库测试或 push 不代表 npm / 市场已经发布。

参考：[Open Computer Use](https://github.com/iFurySt/open-codex-computer-use)、[OpenAI 图像输入协议](https://developers.openai.com/api/docs/guides/images-vision)。

## 创建并打开工作簿

先 get_app_state 获取 app 对应的 observation，再调用 create_spreadsheet。模型工具的参数位于同一层，例如 `{"action":"create_spreadsheet","app":"WPS Office","observation":"<刚返回的 UUID>","filename":"小明年龄表.xlsx","sheet_name":"人员信息","rows":[["姓名","年龄"],["小明",18]]}`，不使用 arguments 包裹。worker 0.8.1 兼容 WPS Office 与系统登记名 wpsoffice 的空格差异，多个应用匹配时仍拒绝启动。worker 只接受标量单元格和等宽行，不接受公式对象、宏或任意路径。每次在本机私有状态目录 documents 下创建独立子目录，因此同名文件不会覆盖。

回执 artifact 包含实际 path、sha256、sheet、rows、columns、verified、opened。verified 表示导出后单元格重新解析通过；opened 表示系统已接受打开请求，仍须核对截图中的窗口和内容。打开或回读失败仍保留已保存文件的回执，不应重派创建。浏览器不直接读取设备路径，用户在本机 WPS 中访问该文件。

## 复合输入指令与验收

“打开wps输入helloworld”整句交给 Responses 工具循环，不截取成应用名。若只要求打开表格应用并输入内容、未指定现有文件或单元格，默认新建内容工作簿：先观察应用，再以 `rows:[["helloworld"]]` 调用 create_spreadsheet，核对导出后重新解析的单元格、实际打开窗口和返回路径。用户指定已有文档时不能以新建文件冒充编辑。

WPS 现有文档编辑仍受 OCU 0.3.5 兼容性限制：type_text 可能无法识别可编辑焦点，set_value 也可能只改变编辑区的临时辅助功能值，回车后并未写入单元格。名称框中的文字、临时 Value 或原生 success 都不足以证明输入完成；必须提交后重新读取目标单元格并核对截图。失败动作已消费 observation，继续前需重新观察。已绑定设备的会话不自动注入跨会话记忆，历史完成记录不能替代本次执行证据。
