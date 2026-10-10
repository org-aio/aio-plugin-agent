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
pub(crate) async fn settings(data: &Value) -> Result {
    view! {
        <section class="dx-conversation-settings">
            <header><h2>"模型服务"</h2>button(action:"provider",label:"添加服务")</header>
            <p>"配置服务地址和 API Key，模型列表会从服务读取。云电脑使用设备上的 Codex 配置。"</p>
            for provider in list(data,"providers") {
                <div class="dx-conversation-settings__row"><div><strong>(text(provider,"model"))</strong><p>(text(provider,"endpoint"))</p></div>
                    button(action:"provider",label:"编辑",value:text(provider,"id")) button(action:"delete-provider",label:"删除",value:text(provider,"id"))
                </div>
            }
            <section><h2>"网页搜索"</h2><p>if data["webSearch"]["enabled"]==true {"Tavily · 已启用"} else {"Tavily · 未启用"}</p>
                button(action:"web-search",label:"配置网页搜索")</section>
        </section>
    }
}

#[component]
pub(crate) async fn dialog(data: &Value) -> Result {
    let kind = text(data, "kind");
    let item = &data["item"];
    view! {
        <div class="dx-dialog-backdrop" data-state="open" data-action="close-dialog"></div>
        <section class="dx-dialog" role="dialog" aria-modal="true" aria-labelledby="dialog-title" tabindex="-1">
            <h2 id="dialog-title" class="dx-dialog-title">(text(data,"title"))</h2>
            <p id="dialog-error" role="alert" hidden=(true)></p>
            match kind {
                "settings" => { settings(data: item) },
                "provider" => {
                    <form data-form="provider" data-id=(text(item,"id"))>
                        field(name:"endpoint",label:"服务地址",value:text(item,"endpoint"),required:true)
                        field(name:"secret",label:"API Key",kind:"password")
                        <p>"密钥留空保留已保存的值。"</p>
                        button(action:"load-provider-models",label:"读取模型")
                        <label>"模型"<select name="model" aria-label="模型" class="dx-select" required=(true)><option value=(text(item,"model"))>(text(item,"model"))</option></select></label>
                        <footer><button class="dx-button" type="submit">"保存"</button></footer>
                    </form>
                },
                "web-search" => {
                    <form data-form="web-search">
                        <label><input type="checkbox" name="enabled" checked=(item["enabled"]==true)>"启用网页搜索"</label>
                        field(name:"secret",label:"Tavily API Key",kind:"password")
                        <label><input type="checkbox" name="clearSecret">"清除已保存的密钥"</label>
                        <button class="dx-button" type="submit">"保存"</button>
                    </form>
                },
                "new" => {
                    <form data-form="new">
                        field(name:"title",label:"对话标题",value:"新对话",required:true)
                        if data["cloud"]!=true&&data["memory"]==true {<label>"记忆空间"<select name="spaceId" class="dx-select" aria-label="记忆空间"><option value="">"个人记忆空间"</option>
                            for space in list(data,"spaces") {<option value=(text(space,"id"))>(text(space,"title"))</option>}
                        </select></label>}
                        <button class="dx-button" type="submit">"保存"</button>
                    </form>
                },
                "confirm" => {
                    <p>(text(data,"description"))</p>button(action:"confirm",label:"确认")
                },
                "questions" => {
                    <form data-form="questions">
                        for question in list(item,"questions") {
                            <fieldset><legend>(text(question,"title"))</legend>
                                if !list(question,"options").is_empty() {
                                    <select class="dx-select" name=(text(question,"id")) aria-label=(text(question,"title"))><option value="">"请选择"</option>
                                        for option in list(question,"options") {<option value=(text(option,"value"))>(text(option,"label"))</option>}
                                    </select>
                                }
                                if question["allowText"]!=false {field(name:format!("text:{}",text(question,"id")),label:format!("{}：填写答案",text(question,"title")))}
                            </fieldset>
                        }
                        <button type="submit" class="dx-button">"提交并继续"</button>button(action:"stop",label:"取消本次任务")
                    </form>
                },
                "approval" => {
                    <pre>(serde_json::to_string_pretty(&item["params"]).unwrap_or_default())</pre>
                    if text(item,"method")=="item/tool/requestUserInput" {
                        <form data-form="native-input">
                            for question in list(&item["params"],"questions") {
                                <fieldset><legend>(text(question,"question"))</legend>
                                    <select class="dx-select" name=(text(question,"id"))><option value="">"请选择"</option>
                                        for option in list(question,"options") {<option value=(text(option,"label"))>(text(option,"label"))" · "(text(option,"description"))</option>}
                                    </select>
                                    field(name:format!("text:{}",text(question,"id")),label:"填写答案")
                                </fieldset>
                            }
                            <button type="submit" class="dx-button">"提交并继续"</button>
                        </form>
                    } else if text(item,"method")=="mcpServer/elicitation/request"&&text(&item["params"],"mode")!="url" {
                        elicitation(data:&item["params"])
                    } else if text(item,"method")=="mcpServer/elicitation/request" {
                        <p>(text(&item["params"],"message"))</p>
                        <p>"复制授权链接并在浏览器打开，完成授权后返回此处继续。"</p>
                        <pre>(text(&item["params"],"url"))</pre>
                        button(action:"copy-authorization",label:"复制授权链接")
                        button(action:"native-approve",label:"已完成授权，继续",value:"accept")
                        button(action:"native-approve",label:"取消授权",value:"decline")
                    } else {
                        button(action:"native-approve",label:"允许一次",value:"accept")
                        button(action:"native-approve",label:"拒绝",value:"decline")
                    }
                },
                "workspace" => {
                    <form data-form="workspace">field(name:"path",label:"项目路径",value:text(item,"path"),required:true)<p>"选择已授权根目录中的项目。"</p><button class="dx-button" type="submit">"选择项目"</button></form>
                },
                "tasks" => {
                    super::tasks::tasks(data:data)
                },
                "skill" => {super::skills::editor(data:data)},
                "source"|"entry"|"spaces"|"grant"|"review" => {super::memory::dialog(data:data)},
                "skills" => {super::skills::library(data:item)},
                _ => {<p>(text(data,"description"))</p>}
            }
            button(action:"close-dialog",label:"关闭")
        </section>
    }
}

/// MCP 的表单由原生 requestedSchema 驱动，审批结果仍交回 Codex。
#[component]
async fn elicitation(data: &Value) -> Result {
    let schema = &data["requestedSchema"];
    let fields = schema["properties"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    view! {
        <p>(text(data,"message"))</p>
        <form data-form="native-elicitation">
            for (name,spec) in &fields {
                <label>(spec["title"].as_str().unwrap_or(name))
                    if text(spec,"type")=="boolean" {
                        <input type="checkbox" name=(name) checked=(spec["default"]==true)>
                    } else if !list(spec,"enum").is_empty()||!list(spec,"oneOf").is_empty() {
                        <select name=(name) class="dx-select-trigger" required=(list(schema,"required").contains(&Value::String(name.clone())))>
                            <option value="">"请选择"</option>
                            for option in list(spec,"enum") {<option value=(option.as_str().unwrap_or_default())>(option.as_str().unwrap_or_default())</option>}
                            for option in list(spec,"oneOf") {<option value=(text(option,"const"))>(text(option,"title"))</option>}
                        </select>
                    } else if ["object","array"].contains(&text(spec,"type")) {
                        <textarea class="dx-textarea" name=(name) aria-label=(name) placeholder="JSON">(spec.get("default").map(Value::to_string).unwrap_or_default())</textarea>
                    } else {
                        <input class="dx-input" name=(name) type=(if ["integer","number"].contains(&text(spec,"type")){"number"}else if text(spec,"format")=="email"{"email"}else{"text"}) step=(if text(spec,"type")=="integer"{"1"}else{"any"}) min=(spec["minimum"].as_f64().map(|n|n.to_string()).unwrap_or_default()) max=(spec["maximum"].as_f64().map(|n|n.to_string()).unwrap_or_default()) value=(spec["default"].as_str().unwrap_or_default()) required=(list(schema,"required").contains(&Value::String(name.clone())))>
                    }
                    <small>(text(spec,"description"))</small>
                </label>
            }
            <button type="submit" class="dx-button">"提交并继续"</button>button(action:"native-approve",label:"拒绝",value:"decline")
        </form>
    }
}
