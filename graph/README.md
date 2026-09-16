# 聊天图谱

使用与 Memory 相同的 az-compose 图谱组件，按 source.lock.json 锁定源码。运行 scripts/prepare-graph.mjs 生成 graph/src；不得手工编辑生成目录。AIO_GRAPH_SOURCE 可指定有读取权限的本地上游 Git 仓库。

默认源码缓存位于用户目录 `.cache/aio/sources/<Git 地址 SHA256>`，只读取锁定的完整提交。私有依赖在交付服务器由维护者通过 `aio-platform/delivery/seed-source.cjs` 导入对应插件的隔离缓存；不把私有源码提交到公开插件仓库，也不向构建容器传递 GitHub 凭据。更新 source.lock.json 时须同步预置新提交。
