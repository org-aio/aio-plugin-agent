# Skill 管理

页面入口为智能体 → Skill 管理。`POST /skills` 接受 list/read/write/resolve；
本机 worker 通过宿主验证身份后调用 `POST /worker/skills.sync`，额外支持 status。
租户与用户来自宿主，设备 ID 来自宿主生成的 worker 上下文，正文不能指定身份。

技能文件保存在 Agent 自有 PostgreSQL schema 内；内容使用宿主提供的 AES-GCM 密钥加密，
AAD 绑定租户、用户和路径。写入以内容哈希比较并交换，旧版本保留在 history。
单文件上限 2 MiB，个人库 32 MiB / 4096 文件。相同库的写入由数据库事务串行化。
同步器每 30 秒比较本地、云端和上次同步版本；冲突需要用户选择，不能用时间戳覆盖。
