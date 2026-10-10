# 浏览器交互

`app.js` 持有页面状态、会话导航和生命周期，`interactions.js` 处理真实 DOM 事件，`dialogs.js` 处理设置、记忆和技能操作。`transport.js` 通过 SDK 获取 Topcoat 片段，按消息缓存、忽略过期响应并保留展开状态。`models.js` 表达跟随空间、跟随服务和显式模型选择。

`cloud.js` 使用 AIO 设备视图票据建立独立 WebSocket，并转发 Codex 原生协议。它不实现模型调用循环。浏览器离开时只关闭自己的连接。路由通过官方 navigation SDK 在 AIO 外层 URL 同步；不读取宿主 cookie 或本地密钥。
