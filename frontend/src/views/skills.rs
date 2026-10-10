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
pub(crate) async fn library(data: &Value) -> Result {
    view! {
        <section class="admin-page"><header><h2>"Skill 管理"</h2><p>"管理技能正文与资源；已连接设备双向同步。"</p>button(action:"refresh-skills",label:"刷新") button(action:"edit-skill",label:"新建 Skill")</header>
            for device in list(data,"devices") {
                <section><h3>(text(&device["report"],"label"))</h3><p>(text(&device["report"],"root"))</p><p>(text(&device["report"],"phase"))" "(text(&device["report"],"error"))</p>
                    for conflict in list(&device["report"],"conflicts") {<div><code>(text(conflict,"path"))</code>
                        for (side,label) in [("local","保留本机"),("remote","保留云端")] {
                            button(action:"resolve-skill",label:label,value:serde_json::json!({"operation":"resolve","device":device["id"],"path":conflict["path"],"side":side,"local":conflict["local"],"remote":conflict["remote"]}).to_string())
                        }
                    </div>}
                </section>
            }
            <input id="skill-search" class="dx-input" aria-label="搜索 Skill" placeholder="搜索 Skill…">
            <table><thead><tr><th>"Skill"</th><th>"操作"</th></tr></thead><tbody>
                for file in list(data,"files").iter().filter(|file|!file["hash"].is_null()&&text(file,"path").ends_with("/SKILL.md")) {
                    <tr data-skill=(text(file,"path"))><td>(text(file,"path").trim_end_matches("/SKILL.md"))</td><td>button(action:"edit-skill",label:"管理",value:text(file,"path"))</td></tr>
                }
            </tbody></table>
            if list(data,"devices").is_empty() {<p>"连接设备以同步 Skill"</p><code>"aio device skills-enable --path ~/.agents/skills"</code>}
            for group in list(data,"nativeSkills") {for skill in list(group,"skills") {<details><summary>(text(skill,"name"))</summary><p>(text(skill,"description"))</p><code>(text(skill,"path"))</code></details>}}
        </section>
    }
}

#[component]
pub(super) async fn editor(data: &Value) -> Result {
    let item = &data["item"];
    view! {
        <form data-form="skill" data-hash=(text(item,"hash"))>
            if text(item,"path").is_empty() {field(name:"path",label:"Skill 文件路径",required:true)} else {
                <label>"技能文件"<select name="path" aria-label="技能文件" class="dx-select">
                    for file in list(data,"files") {<option value=(text(file,"path")) selected=(text(file,"path")==text(item,"path"))>(text(file,"path"))</option>}
                </select></label>
            }
            <label>"正文"<textarea name="content" rows="18" class="dx-textarea" aria-label="Skill 正文">(text(data,"content"))</textarea></label>
            <label><input type="checkbox" name="executable" checked=(item["executable"]==true)>"保留可执行权限"</label>
            <button type="submit" class="dx-button">"保存"</button>
            if !text(item,"path").is_empty() {button(action:"delete-skill",label:"删除当前文件")}
        </form>
    }
}
