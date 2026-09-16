use crate::transport;
use az_agent_model::{SkillContent, SkillFile};
use az_ui_components::{
    admin::{DeleteRecordsDialog, EditorDialog},
    button::{Button, ButtonVariant},
    checkbox::Checkbox,
    input::Input,
    select::{Select, SelectItem},
    textarea::Textarea,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use dioxus::prelude::*;
use serde_json::{Value, json};

#[component]
pub(super) fn SkillEditor(
    file: Option<SkillFile>,
    files: Vec<SkillFile>,
    on_close: Callback<()>,
    on_saved: Callback<()>,
) -> Element {
    let is_new = file.is_none();
    let mut path = use_signal(|| file.as_ref().map(|f| f.path.clone()).unwrap_or_default());
    let mut expected = use_signal(|| file.as_ref().and_then(|f| f.hash.clone()));
    let mut executable = use_signal(|| file.as_ref().is_some_and(|f| f.executable));
    let mut content = use_signal(|| {
        String::from("---\nname: new-skill\ndescription: 说明技能用途\n---\n\n# 使用说明\n")
    });
    let mut loading = use_signal(|| !is_new);
    let mut failure = use_signal(String::new);
    let mut deleting = use_signal(|| false);
    let prefix = file
        .as_ref()
        .map(|f| f.path.split('/').next().unwrap_or_default().to_owned())
        .unwrap_or_default();
    let choices = files
        .into_iter()
        .filter(|f| f.hash.is_some() && f.path.starts_with(&format!("{prefix}/")))
        .collect::<Vec<_>>();
    use_resource(move || async move {
        if is_new {
            return;
        }
        let selected = path();
        loading.set(true);
        failure.set(String::new());
        let result = transport::request::<SkillContent>(
            "POST",
            "/skills",
            json!({"operation":"read","path":selected}),
        )
        .await;
        match result {
            Ok(value) => {
                expected.set(value.file.hash);
                executable.set(value.file.executable);
                let text = value
                    .content
                    .and_then(|s| STANDARD.decode(s).ok())
                    .and_then(|b| String::from_utf8(b).ok());
                if let Some(text) = text {
                    content.set(text);
                    loading.set(false);
                } else {
                    failure.set("这是二进制资源，随设备同步；请在本机编辑。".into());
                }
            }
            Err(error) => failure.set(error),
        }
    });
    rsx! {
        EditorDialog {title:if is_new {"新建 Skill"}else{"管理 Skill"},description:"文件修改会同步到已连接设备。脚本只保存和同步，不会自动运行。",
            on_close,on_saved,
            save:move |_|Box::pin(async move {
                if loading(){return Err("文件尚未加载或不支持文本编辑".into());}
                let result:Value=transport::request("POST","/skills",json!({"operation":"write","path":path(),"expected":expected(),"content":STANDARD.encode(content().as_bytes()),"executable":executable()})).await?;
                let _=result;Ok(())
            }) as az_ui_components::admin::AsyncResult<()>,
            if is_new {
                label {"文件路径",Input {aria_label:"Skill 文件路径",placeholder:"my-skill/SKILL.md",value:path,oninput:move |e:FormEvent|path.set(e.value())}}
            }else{
                label {"技能文件",Select {value:path,aria_label:"技能文件",on_value_change:move |value|path.set(value),options:choices.into_iter().map(|f|SelectItem::new(f.path.clone(),f.path)).collect()}}
            }
            if !failure().is_empty(){p {role:"alert","{failure}"}}
            label {"正文",Textarea {aria_label:"Skill 正文",rows:"18",value:content,disabled:loading(),oninput:move |e:FormEvent|content.set(e.value())}}
            label {Checkbox {checked:Some(az_ui_components::checkbox::checkbox_state(executable())),on_checked_change:move |v|executable.set(bool::from(v))},"保留可执行权限"}
            if !is_new {Button {r#type:"button",variant:ButtonVariant::Destructive,onclick:move |_|deleting.set(true),"删除当前文件"}}
        }
        if deleting() {
            DeleteRecordsDialog {title:"删除 Skill 文件",items:vec![path()],item_label:|p:String|p,
                warning:"删除将同步到设备；本机旧文件会备份。删除 SKILL.md 后该技能不再出现在列表中。",
                on_close:move |_|deleting.set(false),on_deleted:move |_|on_saved.call(()),
                delete:move |p:String|Box::pin(async move {transport::request::<Value>("POST","/skills",json!({"operation":"write","path":p,"expected":expected(),"content":null,"executable":false})).await?;Ok(())}) as az_ui_components::admin::AsyncResult<()>,
            }
        }
    }
}
