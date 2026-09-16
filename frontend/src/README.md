# 页面功能

main 负责启动、共享状态和生成快照轮询；state 管理会话操作；transport 封装 AIO SDK。
conversation/models 负责聊天和会话模型；settings 负责模型 URL/Key 与搜索配置；memory 负责来源、秘密权限与修订；graph 负责知识节点与本轮激活。授权引用只使用服务返回的 ID 与标题。
