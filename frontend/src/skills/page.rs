use super::editor::SkillEditor;
use crate::transport;
use az_agent_model::{SkillFile, SkillLibrary};
use az_ui_components::{
    admin::{EditorDialog, EmptyState, PageHeader, PageSurface, StatusMessage},
    button::{Button, ButtonVariant},
    data_table::{DataTable, DataTableCellContext, DataTableColumn},
    input::Input,
};
use dioxus::prelude::*;
use serde_json::{Value, json};

#[component]
pub fn SkillPage() -> Element {
    let mut library = use_signal(SkillLibrary::default);
    let mut error = use_signal(String::new);
    let mut loaded = use_signal(|| false);
    let mut filter = use_signal(String::new);
    let mut editing = use_signal(|| None::<Option<SkillFile>>);
    let mut resolution = use_signal(|| None::<Value>);
    let mut refresh = use_signal(|| 0_u64);
    use_resource(move || async move {
        let _ = refresh();
        match transport::request::<SkillLibrary>("POST", "/skills", json!({"operation":"list"}))
            .await
        {
            Ok(value) => {
                library.set(value);
                error.set(String::new());
                loaded.set(true);
            }
            Err(message) => error.set(message),
        }
    });
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(5000).await;
            refresh += 1;
        }
    });
    let files = library
        .read()
        .files
        .iter()
        .filter(|f| {
            f.hash.is_some()
                && f.path.ends_with("/SKILL.md")
                && f.path.to_lowercase().contains(&filter().to_lowercase())
        })
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        PageSurface {
            PageHeader {title:"Skill 管理", detail:"管理技能正文与资源；已连接设备每 30 秒双向同步。",
                Button {variant:ButtonVariant::Outline,onclick:move |_|refresh+=1,"刷新"}
                Button {onclick:move |_|editing.set(Some(None)),"新建 Skill"}
            }
            if !error().is_empty() {StatusMessage {error:true,message:error()}}
            for device in library.read().devices.clone() {
                section {key:"{device.id}",class:"admin-section",
                    h2 {{device.report["label"].as_str().unwrap_or("已连接设备").to_owned()}}
                    p {class:"admin-meta", {device.report["root"].as_str().unwrap_or("").to_owned()}}
                    p {role:"status",
                        if device.report["phase"]=="failed" {"同步失败：" {device.report["error"].as_str().unwrap_or("请检查设备").to_owned()}}
                        else if device.report["phase"]=="conflict" {"存在冲突，其他文件继续同步"}
                        else {"最近同步完成"}
                        " · 更新时间：{format_time(device.updated_at)}"
                    }
                    for item in device.report["conflicts"].as_array().cloned().unwrap_or_default() {
                        div {class:"admin-actions",
                            code {{item["path"].as_str().unwrap_or("").to_owned()}}
                            for (side,label) in [("local","保留本机"),("remote","保留云端")] {
                                Button {variant:ButtonVariant::Outline,onclick:{let id=device.id.clone();let item=item.clone();move |_|resolution.set(Some(json!({"operation":"resolve","device":id,"path":item["path"],"side":side,"local":item["local"],"remote":item["remote"]})))},"{label}"}
                            }
                        }
                    }
                }
            }
            if loaded() && library.read().devices.is_empty() {
                EmptyState {title:"连接设备以同步 Skill",detail:"在已配对的电脑上运行一次，随后可在这里编辑并自动同步。",
                    code {"aio-space skills-enable --path ~/.agents/skills"}
                }
            }
            Input {aria_label:"搜索 Skill",placeholder:"搜索 Skill…",value:filter,oninput:move |e:FormEvent|filter.set(e.value())}
            DataTable {
                aria_label:"Skill 列表",rows:files,
                columns:vec![DataTableColumn::leaf("name","Skill"),DataTableColumn::leaf("files","文件数"),DataTableColumn::leaf("actions","操作")],
                row_key:|f:SkillFile|f.path,
                render_cell:move |cell:DataTableCellContext<SkillFile>| {
                    let file=cell.row;
                    match cell.column.key.as_str() {
                        "name"=>rsx!{strong {{file.path.trim_end_matches("/SKILL.md").to_owned()}}},
                        "files"=>{let prefix=file.path.trim_end_matches("SKILL.md");let count=library.read().files.iter().filter(|f|f.hash.is_some() && f.path.starts_with(prefix)).count();rsx!{"{count}"}},
                        _=>rsx!{Button {variant:ButtonVariant::Outline,onclick:move |_|editing.set(Some(Some(file.clone()))),"管理"}},
                    }
                },
            }
            if let Some(file)=editing() {
                SkillEditor {file,files:library.read().files.clone(),on_close:move |_|editing.set(None),on_saved:move |_|{editing.set(None);refresh+=1;}}
            }
            if let Some(request)=resolution() {
                EditorDialog {title:"确认解决冲突",description:"下次同步将按所选版本更新另一端。版本再次变化时会保留冲突，本机旧文件会备份。",submit_label:"确认处理",
                    on_close:move |_|resolution.set(None),on_saved:move |_|{resolution.set(None);refresh+=1;},
                    save:move |_|{let request=request.clone();Box::pin(async move {transport::request::<Value>("POST","/skills",request).await?;Ok(())}) as az_ui_components::admin::AsyncResult<()>},
                    p {"选择后等待设备下一次同步。"}
                }
            }
        }
    }
}
fn format_time(value: i64) -> String {
    // 使用浏览器本地时区，避免把服务端毫秒值直接展示给用户。
    js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(value as f64))
        .to_locale_string("zh-CN", &js_sys::Object::new())
        .into()
}
