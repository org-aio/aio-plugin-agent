# 执行适配

为独立 engine 提供已授权的模型 RequestBuilder 和工具实例。端点、认证和 Unix broker 由 AIO 服务控制；engine 负责流式解码与工具循环。取消通过释放生成 future 同时终止网络请求和工具调用。

生成请求统一通过 Responses；生产调用宿主 `/egress/responses`，本地预览调用已授权基址 `/responses`。前台问答、工具循环与后台记忆整理共用同一协议。
