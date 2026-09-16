# 执行适配

为独立 engine 提供已授权的模型 RequestBuilder 和工具实例。端点、认证和 Unix broker 由 AIO 服务控制；engine 负责流式解码与工具循环。取消通过释放生成 future 同时终止网络请求和工具调用。
