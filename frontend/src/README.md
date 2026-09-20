# 页面功能

main 负责启动、共享状态和生成快照轮询；state 管理会话操作；transport 封装 AIO SDK。
conversation/mod.rs 编排共享 Codex 外观，conversation/sidebar、message、composer 分别负责导航、消息和输入，models 负责会话模型；settings 负责模型 URL/Key 与搜索配置；memory 负责来源、秘密权限与修订；graph 负责知识节点与本轮激活。授权引用只使用服务返回的 ID 与标题。

`user_input.rs` 提供会话设备选择器与待答问题 Dialog。问题来自后端的持久化 `pendingInput`，关闭窗口不提交默认答案。回答与取消通过正常宿主桥接口执行；不要把普通聊天消息当成结构化问题的隐式答案。
