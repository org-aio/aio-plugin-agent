# Skill 页面

在独立 skills.html 中挂载 SkillPage，使用共享 PageSurface、DataTable、EditorDialog。
技能目录、资源编辑、设备同步状态和双端冲突由 POST /skills 提供；页面只显示当前用户的数据。
新建、编辑、删除与冲突处理均通过 Dialog，编辑携带读取时的哈希，防止覆盖并发修改。
