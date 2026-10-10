use super::{
    list, text,
    widgets::{button, icon, select},
};
use serde_json::Value;
use topcoat::{
    Result,
    view::{Unescaped, component, view},
};

#[component]
pub(crate) async fn history(data: &Value) -> Result {
    let mut groups: Vec<(String, Vec<&Value>)> = list(data, "spaces")
        .iter()
        .map(|space| {
            (
                text(space, "title").into(),
                list(data, "conversations")
                    .iter()
                    .filter(|item| item["spaceId"] == space["id"])
                    .collect(),
            )
        })
        .collect();
    groups.push((
        "对话".into(),
        list(data, "conversations")
            .iter()
            .filter(|item| {
                !list(data, "spaces")
                    .iter()
                    .any(|space| item["spaceId"] == space["id"])
            })
            .collect(),
    ));
    view! {
        for (title,items) in groups.iter().filter(|(_,items)|!items.is_empty()) {
            <details class="dx-conversation__history-group" open=(true)><summary>icon(name:"chevron") icon(name:"folder")<span>(title)</span></summary>
                for item in items {
                    <div class="dx-conversation__history-row" data-selected=(text(item,"id")==text(data,"selected"))>
                        button(action:"select",label:text(item,"title"),value:text(item,"id"))
                        button(action:"delete-thread",label:format!("{}会话 {}",if data["native"]==true {"归档"}else{"删除"},text(item,"title")),value:text(item,"id"),icon_only:true)
                    </div>
                }
            </details>
        }
        if list(data,"conversations").is_empty() { <p class="dx-conversation__no-results">"没有匹配的对话"</p> }
        if data["more"]==true {button(action:"more-history",label:"更多历史会话")}
    }
}

// Markdown 先净化再放入 HTML，原始消息永远不能变成脚本或事件属性。
fn markdown(content: &str, citations: &[Value]) -> Unescaped<String> {
    use pulldown_cmark::{CowStr, Event, Tag, TagEnd};
    let safe_url = |url: CowStr<'_>| {
        if url.starts_with("https://")
            || url.starts_with("http://")
            || url.starts_with("mailto:")
            || (url.starts_with('#') && !url.starts_with("#memory:"))
        {
            url.into_string()
        } else {
            "#".into()
        }
    };
    let mut links = Vec::new();
    let mut replacing = false;
    let mut reference_label = None;
    let parser = pulldown_cmark::Parser::new_ext(content, pulldown_cmark::Options::all())
        .filter_map(|event| {
            if replacing {
                if matches!(event, Event::End(TagEnd::Link)) {
                    replacing = false;
                    reference_label = None;
                    links.pop();
                    return Some(event);
                }
                return reference_label.take().map(Event::Text);
            }
            Some(match event {
                Event::Html(value) | Event::InlineHtml(value) => Event::Text(value),
                Event::Start(Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                }) => {
                    let memory_id = dest_url.strip_prefix("memory:");
                    let trusted = memory_id
                        .is_some_and(|id| citations.iter().any(|item| text(item, "id") == id));
                    let allowed =
                        trusted || (memory_id.is_none() && safe_url(dest_url.clone()) != "#");
                    links.push(allowed);
                    if !allowed {
                        return None;
                    }
                    if trusted {
                        replacing = true;
                        reference_label = citations
                            .iter()
                            .find(|item| Some(text(item, "id")) == memory_id)
                            .map(|item| CowStr::from(text(item, "title").to_owned()));
                    }
                    Event::Start(Tag::Link {
                        link_type,
                        dest_url: if trusted {
                            format!("#memory:{}", memory_id.unwrap_or_default()).into()
                        } else {
                            safe_url(dest_url).into()
                        },
                        title,
                        id,
                    })
                }
                Event::End(TagEnd::Link) if links.pop() == Some(false) => return None,
                Event::Start(Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                }) => Event::Start(Tag::Image {
                    link_type,
                    dest_url: safe_url(dest_url).into(),
                    title,
                    id,
                }),
                other => other,
            })
        });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    Unescaped::new_unchecked(html)
}

fn status(item: &Value) -> &str {
    match text(item, "status") {
        "generating" | "inProgress" => "回复中",
        "awaiting_input" => "等待回答",
        "queued" => "整理中",
        "cancelled" => "已停止",
        "failed" => "失败",
        "interrupted" => "已中断",
        _ if text(item, "memoryStatus") == "complete" => "已记住",
        _ => "",
    }
}

#[component]
pub(crate) async fn messages(data: &Value) -> Result {
    let header =
        serde_json::json!({"earlier":data["earlier"],"empty":list(data,"messages").is_empty()});
    view! {
        message_header(data: &header)
        for item in list(data,"messages") { message(data:item) }
        message_footer(data:data)
    }
}

#[component]
pub(crate) async fn message_header(data: &Value) -> Result {
    view! {
        if data["earlier"]==true {button(action:"earlier",label:"加载更早的消息")}
        if data["empty"]==true {<div class="dx-conversation__empty"><h1>"今天想完成什么？"</h1></div>}
    }
}

#[component]
pub(crate) async fn message(data: &Value) -> Result {
    let item = data;
    view! {
            <article class="dx-conversation__message" data-role=(text(item,"role")) data-id=(text(item,"id"))>
                if !status(item).is_empty() {<header><span role="status">(status(item))</span></header>}
                <div class="dx-conversation__message-body dx-markdown">
                    (markdown(text(item,"content"),list(item,"citations")))
                    if !text(item,"error").is_empty() { <p role="alert">(text(item,"error"))</p> }
                </div>
                for source in list(item,"images") {if let Some(url)=source.as_str().filter(|url|url.starts_with("https://")||url.starts_with("http://")) {<img src=(url) alt="任务图片">}}
                if let Some(details)=item.get("details") { <details><summary>"执行详情"</summary><pre>(serde_json::to_string_pretty(details).unwrap_or_default())</pre></details> }
                <footer class="dx-conversation__message-actions">
                    button(action:"copy-message",label:if text(item,"role")=="user" {"复制消息"}else{"复制回复"},value:text(item,"id"),icon_only:true)
                    if item["historyActions"]==true {
                        button(action:"native-fork",label:"从此处分支",value:text(item,"turnId"))
                        button(action:"native-revert",label:"回退到此处",value:text(item,"turnId"))
                    }
                    if !text(item,"sourceId").is_empty() { button(action:"source",label:"来源资料",value:text(item,"sourceId")) }
                    for source in list(item,"citations") { button(action:"entry",label:text(source,"title"),value:text(source,"id")) }
                    if !list(item,"activatedNodeIds").is_empty() {button(action:"graph",label:"查看关联",value:text(item,"id"))}
                    if let Some(tokens)=item["tokens"].as_u64() {<small>(tokens)" tokens"</small>}
                    if text(item,"memoryStatus")=="pending"||text(item,"memoryStatus")=="processing" { <small>"正在整理记忆…"</small> }
                </footer>
            </article>
    }
}

#[component]
pub(crate) async fn message_footer(data: &Value) -> Result {
    view! {
        if !data["pendingInput"].is_null() { <section aria-label="等待回答"><strong>"需要你补充信息，任务已暂停"</strong>button(action:"questions",label:"回答问题")</section> }
        for approval in list(data,"approvals") {
            <section aria-label="等待审批"><strong>"Codex 请求确认"</strong><pre>(serde_json::to_string_pretty(&approval["params"]).unwrap_or_default())</pre>
                button(action:"approval",label:"处理请求",value:approval["id"].to_string())
            </section>
        }
    }
}

#[component]
pub(crate) async fn controls(data: &Value) -> Result {
    let devices:Vec<Value>=list(data,"devices").iter().map(|device|serde_json::json!({"id":device["id"],"label":format!("{} · {}",text(device,"label"),if text(device,"status")=="online" {"在线"}else{"离线"}),"disabled":text(device,"status")!="online"})).collect();
    view! {
        <div class="dx-conversation__model-selector">select(name:"target",label:"执行设备",placeholder:"对话",items:&devices,selected:text(data,"target"))
        button(action:"refresh-devices",label:"刷新设备",icon_only:true)</div>
        <div class="dx-conversation__model-selector">select(name:"workspace",label:"本地项目",placeholder:"项目",items:list(data,"workspaces"),selected:text(data,"workspace"),disabled:text(data,"target").is_empty())
        button(action:"add-workspace",label:"添加本地项目",disabled:text(data,"target").is_empty(),icon_only:true)</div>
    }
}

#[component]
pub(crate) async fn models(data: &Value) -> Result {
    view! { <div class="dx-conversation__model-selector">select(name:"model",label:"对话模型",placeholder:"跟随默认模型",items:list(data,"models"),selected:text(data,"model"))</div> }
}

#[component]
pub(crate) async fn shortcuts(data: &Value) -> Result {
    view! {
        <span class="dx-conversation__shortcut-label">"可用模型"</span>
        for model in list(data,"models").iter().take(8) {
            <button class="dx-button" type="button" data-style="ghost" data-action="model" data-value=(text(model,"id")) aria-label=(format!("切换模型 {}",text(model,"label"))) aria-pressed=(text(model,"id")==text(data,"model")) title=(text(model,"label"))>(text(model,"label").split(" · ").next().unwrap_or_default())</button>
        }
        button(action:"refresh-models",label:"刷新模型列表",icon_only:true)
    }
}
