# 正式宿主

通过 AIO_PLUGIN_CONFIG 读取 v2 宿主私有配置，以 AIO_PLUGIN_SOCKET 提供 Unix 服务入口。数据库、主密钥和访问票据只存在 Rust 服务；模型执行器仍只接收净化资料。模型与 Memory 请求经过宿主 Unix broker，宿主重新校验活动版本、租户和交互身份。
