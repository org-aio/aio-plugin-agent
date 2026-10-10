use super::widgets::button;
use topcoat::{Result, view::view};

pub(crate) async fn document(page: &str) -> Result {
    let cx = &topcoat::context::Cx::default();
    view! { cx =>
        <!DOCTYPE html>
        <html lang="zh-CN"><head>
            <meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover,interactive-widget=resizes-content">
            <meta name="aio-page" content=(page)><title>"AIO 智能体"</title>
            <link rel="stylesheet" href="styles/theme.css"><link rel="stylesheet" href="styles/admin/theme.css">
            <link rel="stylesheet" href="styles/admin/style.css"><link rel="stylesheet" href="styles/utilities.css">
            <link rel="stylesheet" href="styles/button/style.css"><link rel="stylesheet" href="styles/input/style.css">
            <link rel="stylesheet" href="styles/textarea/style.css"><link rel="stylesheet" href="styles/select/style.css">
            <link rel="stylesheet" href="styles/dialog/style.css"><link rel="stylesheet" href="styles/markdown/style.css">
            <link rel="stylesheet" href="styles/conversation/style.css"><link rel="stylesheet" href="styles/conversation/codex.css">
            <link rel="stylesheet" href="styles/conversation/codex-panels.css"><link rel="stylesheet" href="styles/spatial/style.css">
            <script type="module" src="app.js"></script>
        </head><body>
            if page == "chat" {
                <main class="dx-conversation" data-appearance="codex" data-history="false" data-context="false" data-sidebar-collapsed="false">
                    <aside class="dx-conversation__history" aria-label="会话列表">
                        <div class="dx-conversation__sidebar-heading"><span class="dx-conversation__brand">"AIO"</span>button(action:"sidebar",label:"收起会话列表",icon_only:true)</div>
                        <div class="dx-conversation__navigation">
                            button(action:"new",label:"新对话") button(action:"search",label:"搜索对话")
                            button(action:"spaces",label:"记忆空间") button(action:"skills",label:"Skill 管理")
                            <div data-native="workspace" hidden=(true)>button(action:"native-archives",label:"已归档会话")</div>
                        </div>
                        <input id="history-search" class="dx-input" aria-label="搜索会话" placeholder="搜索会话" hidden=(true)>
                        <p class="dx-conversation__section-label">"空间与对话"</p>
                        <nav id="history" aria-label="历史会话" data-url-scroll="agent-history"></nav>
                        <footer class="dx-conversation__sidebar-footer">button(action:"settings",label:"设置")</footer>
                    </aside>
                    <button class="dx-conversation__scrim" hidden=(true) type="button" data-action="sidebar" aria-label="关闭会话列表"></button>
                    <section class="dx-conversation__main" aria-label="对话" data-empty="true">
                        <header class="dx-conversation__header">
                            button(action:"sidebar",label:"切换会话列表",icon_only:true)<span id="title" class="dx-conversation__title">"新对话"</span>
                            <div class="dx-conversation__toolbar"><span data-native="workspace" hidden=(true)>button(action:"native-files",label:"项目文件",icon_only:true)</span>button(action:"environment",label:"环境信息",icon_only:true) button(action:"graph",label:"知识图谱",icon_only:true) button(action:"new",label:"新对话",icon_only:true)</div>
                        </header>
                        <button id="reconnect" type="button" class="dx-button" data-action="reconnect" hidden=(true)>"重新连接云电脑"</button>
                        <p id="error" class="dx-conversation__error" role="alert" hidden=(true)></p>
                        <div id="messages" class="dx-conversation__messages" aria-live="polite" data-url-scroll="agent-messages"><div class="dx-conversation__empty"><h1>"今天想完成什么？"</h1></div></div>
                        <div class="dx-conversation__composer-wrap">
                            <div id="model-shortcuts" class="dx-conversation__model-shortcuts"></div>
                            <form id="composer" class="dx-conversation__composer">
                                <textarea id="draft" class="dx-textarea" name="content" rows="2" aria-label="发送消息" placeholder="随心输入" title="Enter 发送，Shift + Enter 换行"></textarea>
                                <div class="dx-conversation__composer-actions">
                                    <div class="dx-conversation__more"><button type="button" data-action="more" aria-label="更多选项" aria-expanded="false">"＋"</button><div id="conversation-actions" class="dx-conversation__action-menu" hidden=(true)><div data-native="thread" hidden=(true)>button(action:"native-rename",label:"重命名会话") button(action:"native-fork",label:"新建分支会话") button(action:"native-compact",label:"压缩上下文")</div><div class="dx-conversation__mobile-actions">button(action:"refresh-devices",label:"刷新设备") button(action:"add-workspace",label:"添加本地项目") button(action:"refresh-models",label:"刷新模型列表")</div>button(action:"tasks",label:"蜂群任务") button(action:"spaces",label:"记忆空间") button(action:"settings",label:"模型与工具设置")</div></div>
                                    <div id="controls" class="dx-conversation__device-control"></div><div id="model-control" class="dx-conversation__model-control"></div>
                                    <button id="send" type="submit" class="dx-button dx-conversation__send" aria-label="发送">"↑"</button>
                                </div>
                            </form>
                            <div class="dx-conversation__composer-footer"><small id="status" role="status">"正在连接…"</small></div>
                        </div>
                    </section>
                    <aside id="panel" class="dx-conversation__context" hidden=(true)></aside>
                </main>
            } else {
                <main id="page" class="dx-conversation-settings"><p>"正在加载…"</p></main>
                <p id="error" role="alert" hidden=(true)></p>
            }
            <div id="dialogs"></div>
        </body></html>
    }
}
