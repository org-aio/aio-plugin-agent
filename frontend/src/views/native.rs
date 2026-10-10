use super::{
    list, text,
    widgets::{button, field},
};
use serde_json::Value;
use topcoat::{
    Result,
    view::{component, view},
};

// 原生文件和会话数据只参与渲染，操作仍由设备连接提交给 Codex。
#[component]
pub(super) async fn dialog(data: &Value) -> Result {
    view! {
        if data["loading"]==true {<p role="status">"正在读取云电脑…"</p>}
        if !text(data,"error").is_empty() {<p role="alert">(text(data,"error"))</p>}
        match text(data,"kind") {
            "native-rename" => {
                <form data-form="native-rename" data-id=(text(&data["item"],"id"))>
                    field(name:"name",label:"会话名称",value:text(&data["item"],"name"),required:true)
                    <button class="dx-button" type="submit">"保存"</button>
                </form>
            },
            "native-files" => {
                <p class="dx-conversation__source">(text(data,"path"))</p>
                if !text(data,"parent").is_empty() {button(action:"native-files",label:"上一级目录",value:text(data,"parent"))}
                if data["file"]==true {
                    <div class="dx-markdown"><pre><code>(text(data,"content"))</code></pre></div>
                } else {
                    <nav aria-label="项目文件列表">
                        for item in list(data,"items") {
                            <div class="dx-conversation-settings__row">
                                button(action:if item["isDirectory"]==true {"native-files"}else{"native-file"},label:format!("{}{}",text(item,"fileName"),if item["isDirectory"]==true {"/"}else{""}),value:text(item,"path"))
                            </div>
                        }
                    </nav>
                    if data["loading"]!=true&&text(data,"error").is_empty()&&list(data,"items").is_empty() {<p>"目录为空"</p>}
                }
            },
            "native-archives" => {
                for item in list(data,"items") {
                    <div class="dx-conversation-settings__row"><span>(item["name"].as_str().or(item["preview"].as_str()).unwrap_or("新对话"))</span>button(action:"native-restore",label:"恢复会话",value:text(item,"id"))</div>
                }
                if data["loading"]!=true&&text(data,"error").is_empty()&&list(data,"items").is_empty() {<p>"当前项目没有已归档会话"</p>}
                if !text(data,"cursor").is_empty() {button(action:"native-more-archives",label:"更多已归档会话")}
            },
            _ => {}
        }
    }
}
