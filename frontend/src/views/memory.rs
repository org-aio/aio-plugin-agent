use super::{
    list, text,
    widgets::{button, field},
};
use serde_json::Value;
use topcoat::{
    Result,
    view::{component, view},
};

#[component]
pub(crate) async fn panel(data: &Value) -> Result {
    view! {

            <header><strong>(text(data,"title"))</strong>button(action:"refresh-panel",label:"刷新") button(action:"close-panel",label:if text(data,"kind")=="graph" {"收起图谱".into()}else{format!("关闭{}",text(data,"title"))},icon_only:true)</header>
            if text(data,"kind")=="environment" {
                <dl>for item in list(data,"items") {<dt>(text(item,"label"))</dt><dd>(text(item,"value"))</dd>}</dl>
            } else {
                <div class="dx-conversation__graph-toolbar">button(action:"graph-layout",label:"图谱 / 列表") button(action:"graph-zoom",label:"放大",value:"1.2") button(action:"graph-zoom",label:"缩小",value:"0.8")</div>
                <svg class="dx-conversation__graph" viewBox="0 0 600 500" role="img" aria-label="记忆关系图" id="memory-graph">
                    for edge in list(data,"edges") {
                        <line x1=(edge["x1"].as_f64().unwrap_or_default()) y1=(edge["y1"].as_f64().unwrap_or_default()) x2=(edge["x2"].as_f64().unwrap_or_default()) y2=(edge["y2"].as_f64().unwrap_or_default()) stroke="currentColor" opacity="0.3"></line>
                    }
                    for node in list(data,"nodes") {
                        <g class="dx-conversation__graph-node" data-active=(node["active"]==true) data-matched=(node["matched"]==true) data-action="entry" data-value=(text(node,"id")) tabindex="0" role="button" aria-label=(text(node,"title"))>
                            <circle cx=(node["x"].as_f64().unwrap_or(300.0)) cy=(node["y"].as_f64().unwrap_or(250.0)) r="12" fill="currentColor"></circle>
                            <text x=(node["x"].as_f64().unwrap_or(300.0)+16.0) y=(node["y"].as_f64().unwrap_or(250.0)) fill="currentColor">(text(node,"title"))</text>
                        </g>
                    }
                </svg>
                <div id="graph-list" hidden=(true)>for node in list(data,"nodes") {button(action:"entry",label:text(node,"title"),value:text(node,"id"))}</div>
                if list(data,"nodes").is_empty() {<p>"暂无记忆"</p>}
                if data["focused"]==true {button(action:"latest-graph",label:"最新一轮")}
            }
    }
}

#[component]
pub(super) async fn dialog(data: &Value) -> Result {
    let item = &data["item"];
    view! {
        match text(data,"kind") {
            "source" => {
                <p class="dx-conversation__source">(text(item,"text"))</p>
                if !text(item,"error").is_empty() {<p role="alert">(text(item,"error"))</p>}
                for secret in list(item,"secrets") {
                    <section data-secret=(text(secret,"id"))><h3>(text(secret,"label"))</h3><p data-secret-value="true">"••••••••"</p>
                        button(action:"reveal-secret",label:"查看秘密",value:text(secret,"id"),disabled:secret["canReveal"]!=true)
                        button(action:"copy-secret",label:"复制秘密",value:text(secret,"id"))
                        if secret["canManage"]==true {button(action:"grant",label:"秘密授权",value:text(secret,"id"))}
                    </section>
                }
                if text(item,"status")=="conflict" {button(action:"review",label:"核实修订",value:text(item,"id"))}
                if text(item,"status")=="failed" {button(action:"retry-source",label:"重新整理",value:text(item,"id"))}
            },
            "entry" => {
                <p class="dx-conversation__source">(text(item,"content"))</p>
                for source in list(data,"sources") {button(action:"source",label:text(source,"text"),value:text(source,"id"))}
            },
            "grant" => {
                <form data-form="grant" data-id=(text(item,"id"))>
                    field(name:"userId",label:"空间成员 ID",required:true)
                    <label><input type="checkbox" name="reveal" checked=(true)>"允许查看与复制"</label>
                    <label><input type="checkbox" name="manage">"允许管理授权"</label>
                    <button type="submit" class="dx-button">"保存"</button>
                </form>
            },
            "spaces" => {
                <form data-form="spaces" data-id=(text(item,"id"))>
                    <select name="space" class="dx-select" aria-label="管理记忆空间"><option value="">"新建空间"</option>
                        for space in list(data,"spaces") {<option value=(text(space,"id")) selected=(text(space,"id")==text(item,"id"))>(text(space,"title"))</option>}
                    </select>
                    field(name:"title",label:"空间名称",value:text(item,"title"),required:true)
                    <label>"整理模型"<select name="modelBinding" class="dx-select" aria-label="整理模型"><option value="">"暂不整理"</option>
                        for provider in list(data,"providers") {<option value=(text(provider,"id")) selected=(text(provider,"id")==text(item,"modelBinding"))>(text(provider,"model"))</option>}
                    </select></label>
                    <button type="submit" class="dx-button" disabled=(!text(item,"id").is_empty()&&text(item,"role")!="OWNER")>"保存"</button>
                </form>
                for member in list(data,"members") {
                    <div><span>(text(member,"userId"))" · "(text(member,"role"))</span>if text(item,"role")=="OWNER" {button(action:"remove-member",label:"移除成员",value:text(member,"userId"))}</div>
                }
                if !text(item,"id").is_empty()&&text(item,"role")=="OWNER" {
                    <form data-form="member" data-id=(text(item,"id"))>
                        field(name:"userId",label:"添加成员 ID",required:true)
                        <select name="role" aria-label="成员角色"><option value="READER">"只读"</option><option value="EDITOR">"编辑"</option><option value="OWNER">"所有者"</option></select>
                        <button type="submit" class="dx-button">"添加成员"</button>
                    </form>
                }
            },
            "review" => {
                for entry in list(item,"entries") {<section><h3>(text(&entry["draft"],"title"))</h3><p>"当前："(text(&entry["current"],"content"))</p><p>"建议："(text(&entry["draft"],"content"))</p></section>}
                button(action:"resolve-review",label:"接受修订",value:"true") button(action:"resolve-review",label:"保留现有内容",value:"false")
            },
            _ => {}
        }
    }
}
