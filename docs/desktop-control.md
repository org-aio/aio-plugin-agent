# 通用桌面操作

Agent → AIO 宿主授权桥 → 已配对 aio-space worker → Open Computer Use MCP → 本机桌面。结果沿原任务通道返回；界面和模型都能收到截图。模型仍在 Agent 中运行，worker 负责操作系统输入与观察，不在每台设备另起模型。

## 开启与升级

1. 宿主先升级到支持 `desktop.control` 的版本；Agent 清单声明该能力并通过插件授权流程取得权限。
2. 本机安装包含桌面适配器的 aio-space，沿用现有配对。运行 `aio-space desktop-enable` 后再以 `aio-space worker --background` 更新后台入口。
3. 在 macOS 系统设置中授予实际 worker / OCU 运行程序辅助功能与屏幕录制权限。保持图形用户会话已登录、桌面可操作。OCU 自带 `ocu doctor` 可检查原生权限。
4. 选择支持图像输入和工具调用的模型，以及会话执行设备。已有“打开应用”或工作区权限不会自动扩大为桌面控制权限。

`aio-space desktop-status` 查询本机开关；`aio-space desktop-disable` 先关闭本机接收，再撤销服务端能力并取消未完成桌面任务。网络失败应重试服务端撤销。该授权覆盖本机账号可以通过桌面完成的操作，包括前台鼠标和键盘输入；它不是应用沙箱，也不受工作区目录边界限制。

## 执行契约

- `desktop_control` 提供 list_apps、get_app_state、activate_app、click、type_text、press_key、scroll、drag、set_value、perform_secondary_action、release、wait。底层 MCP 工具固定，不接受远程可执行文件或 shell 参数。
- 会话 UUID 由后端绑定。一台 worker 同时只允许一个会话使用桌面；空闲 120 秒关闭 MCP，会话完成可主动 release。其他人手动操作桌面仍会影响界面，操作前须重新观察。
- get_app_state 返回截图、元素索引及 observation；动作必须使用相同应用的最新凭据。凭据有效期 120 秒且在副作用前消费一次，失败、断网或取消后不能用旧凭据重放。
- 动作后自动回读界面。工具 `success` 只说明原生调用及回读成功；“已创建”“已输入”“已保存”必须由界面内容或文件证据验证。queued/running 使用原 task_id 等待，不重新派发。
- 任务沿用设备租约与取消。模型轮数上限 32，总时限至少 600 秒；超出预算返回实际未完成状态。截图和界面文字都是不可信资料，不替代用户指令。
- 截图压缩为 JPEG，保留原始像素尺寸，避免坐标错位；超过 270 KB 时省略并明确说明，不能猜测坐标。图片以真正的多模态消息传给模型，不把 base64 放进文本工具结果。模型不支持图像时需更换模型；不会自动换供应商。
- Agent 加密保存回执及截图；任务界面只返回最新一张图片，历史截图以文字说明代替。刷新仍能读取持久化结果。

## 已验证与限制（2026-09-17）

模拟 MCP 测试覆盖观察凭据、会话独占、失败后不重放、图片尺寸与权限。模拟 HTTP/SSE 验证多工具图片顺序、旧图释放和暂停恢复。隔离 PostgreSQL 覆盖设备授权、租户隔离、任务查询和撤权取消；真实 wasm 页面在 1440×900 与 390×844 显示图片、刷新保留回执且无横向溢出。

macOS 上 OCU 0.3.5 的辅助功能和录屏检查通过，能够列举 WPS、读取辅助功能树并返回 1280×746 截图。但本次 WPS 首页“新建”点击及快捷键尚未观察到可验证的页面变化，不能宣称已经完成 WPS 建表或保存。需要继续排查原生输入与 WPS 的兼容性。activate_app 复用现有 macOS 启动器；Windows/Linux 未进行端到端验收。

本改动不是生产上线证明。宿主、Agent 和 worker 必须配套升级并完成本机授权；仓库测试或 push 不代表 npm / 市场已经发布。

参考：[Open Computer Use](https://github.com/anomalyco/computer-use)、[OpenAI 图像输入协议](https://developers.openai.com/api/docs/guides/images-vision)。
