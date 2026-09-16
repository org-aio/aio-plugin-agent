use crate::{
    state::{self, AgentState, Dialog as PageDialog},
    transport,
};
use az_agent_model::*;
use az_ui_components::{
    admin::{DeleteRecordsDialog, EditorDialog},
    button::{Button, ButtonVariant},
    checkbox::Checkbox,
    dialog::{Dialog, DialogTitle},
    input::Input,
    select::{Select, SelectItem},
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use uuid::Uuid;
#[component]
pub fn SettingsPanel() -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let settings = state.read().settings.clone();
    rsx! {
        section { class: "dx-conversation-settings",
            header {
                h2 { "模型服务" }
                Button { onclick: move | _ | state.write().dialog =
                            Some(PageDialog::Provider(None)),
                    "添加服务"
                }
            }
            p { class: "admin-meta", "配置服务地址和 API Key，模型列表会从服务读取。" }
            if let Some(settings) = settings {
                if settings.providers.is_empty() {
                    p { "尚未配置模型服务。" }
                }
                for provider in settings.providers {
                    div {
                        key: "{provider.id}",
                        class: "dx-conversation-settings__row",
                        div {
                            strong { "{provider.model}" }
                            p { "{provider.endpoint}" }
                        }
                        Button {
                            variant: ButtonVariant::Outline,
                            onclick: {
                                let provider = provider.clone();
                                move |_| {
                                    state.write().dialog = Some(PageDialog::Provider(Some(provider.clone())));
                                }
                            },
                            "编辑"
                        }
                        Button {
                            variant: ButtonVariant::Ghost,
                            onclick: {
                                let title = provider.label.clone();
                                let path = format!("/providers/{}", provider.id);
                                move |_| {
                                    state.write().dialog = Some(PageDialog::Delete {
                                        title: title.clone(),
                                        path: path.clone(),
                                    });
                                }
                            },
                            "删除"
                        }
                    }
                }
                WebSearchSettings { settings: settings.web_search }
            }
            if let Some(error) = state.read().error.clone() {
                p { role: "alert", "{error}" }
            }
        }
    }
}
#[component]
fn WebSearchSettings(settings: SearchSettings) -> Element {
    let mut editing = use_signal(|| false);
    rsx! {
        section {
            header {
                div {
                    h2 { "网页搜索" }
                    p { class: "admin-meta",
                        "Tavily · "
                        if settings.enabled {
                            "已启用"
                        } else {
                            "未启用"
                        }
                    }
                }
                Button {
                    variant: ButtonVariant::Outline,
                    onclick: move |
                            _ | editing.set(true),
                    "配置网页搜索"
                }
            }
            p {
                if settings.has_secret {
                    "API Key 已保存"
                } else {
                    "尚未配置 API Key"
                }
            }
            if editing() {
                SearchEditor { settings, on_close: move |_| editing.set(false) }
            }
        }
    }
}
#[component]
fn SearchEditor(settings: SearchSettings, on_close: Callback<()>) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut secret = use_signal(String::new);
    let mut enabled = use_signal(|| settings.enabled);
    let mut clear = use_signal(|| false);
    rsx! {
        EditorDialog {
            title: "网页搜索",
            description: "设置仅对当前工作区中的你生效。留空保留已保存的密钥。",
            on_close,
            on_saved: move |_| on_close.call(()),
            save: move |_| {
                Box::pin(async move {
                    let secret = if clear() {
                        Some(String::new())
                    } else if secret().is_empty() {
                        None
                    } else {
                        Some(secret())
                    };
                    let result: SearchSettings = transport::request(
                            "PUT",
                            "/tools/web-search",
                            json!(SearchSettingsDraft { enabled : enabled(), secret }),
                        )
                        .await?;
                    if let Some(settings) = state.write().settings.as_mut() {
                        settings.web_search = result;
                    }
                    Ok(())
                }) as az_ui_components::admin::AsyncResult<()>
            },
            label {
                "Tavily API Key"
                Input {
                    r#type: "password",
                    aria_label: "Tavily API Key",
                    autocomplete: "new-password",
                    value: secret,
                    disabled: clear(),
                    oninput: move | e :
                            FormEvent | secret.set(e.value()),
                }
            }
            label {
                Checkbox {
                    checked: Some(az_ui_components::checkbox::checkbox_state(enabled())),
                    on_checked_change: move |value| enabled.set(bool::from(value)),
                }
                "启用网页搜索"
            }
            if settings.has_secret {
                label {
                    Checkbox {
                        checked: Some(az_ui_components::checkbox::checkbox_state(clear())),
                        on_checked_change: move |value| {
                            let value = bool::from(value);
                            clear.set(value);
                            if value {
                                enabled.set(false)
                            }
                        },
                    }
                    "清除已保存的密钥"
                }
            }
        }
    }
}
#[component]
pub fn Dialogs(dialog: PageDialog) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let close = Callback::new(move |_: ()| state.write().dialog = None);
    match dialog {
        PageDialog::Settings => {
            rsx! {
                Dialog {
                    open: true,
                    class: "dx-conversation-settings-dialog",
                    on_open_change: move |open: bool| {
                        if !open {
                            state.write().dialog = None;
                        }
                    },
                    DialogTitle { "智能体设置" }
                    SettingsPanel {}
                    Button {
                        variant: ButtonVariant::Outline,
                        onclick: move |_| close.call(()),
                        "关闭"
                    }
                }
            }
        }
        PageDialog::Provider(provider) => {
            rsx! {
                ProviderEditor { provider }
            }
        }
        PageDialog::New => {
            rsx! {
                NewConversation {}
            }
        }
        PageDialog::Delete { title, path } => {
            rsx! {
                DeleteRecordsDialog {
                    items: vec![title.clone()],
                    item_label: move |
                                    value : String | value,
                    title: format!("删除 {title}？"),
                    warning: "删除后无法恢复。",
                    on_close: close,
                    on_deleted: move |_| state.write().dialog = None,
                    delete: move |_| {
                        let path = path.clone();
                        Box::pin(async move {
                            let _: Value = transport::request("DELETE", &path, Value::Null).await?;
                            if state
                                .peek()
                                .thread
                                .as_ref()
                                .is_some_and(|t| path == format!("/conversations/{}", t.conversation.id))
                            {
                                state.write().thread = None;
                                state.write().generation += 1;
                            }
                            state::load(state, true).await
                        }) as az_ui_components::admin::AsyncResult<()>
                    },
                }
            }
        }
        PageDialog::Source(source) => {
            rsx! {
                crate::memory::SourceDialog { source }
            }
        }
        PageDialog::Entry { id, title, content } => {
            rsx! {
                crate::memory::EntryDialog { id, title, content }
            }
        }
        PageDialog::Spaces => {
            rsx! {
                crate::memory::SpacesDialog {}
            }
        }
    }
}
#[component]
fn ProviderEditor(provider: Option<Provider>) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let id = provider.as_ref().map(|p| p.id);
    let mut endpoint = use_signal(|| {
        provider
            .as_ref()
            .map(|p| p.endpoint.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".into())
    });
    let mut secret = use_signal(String::new);
    let mut selected = use_signal(|| {
        provider
            .as_ref()
            .map(|p| p.model.clone())
            .unwrap_or_default()
    });
    let mut models = use_signal(Vec::<String>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut loaded = use_signal(|| None::<(String, String)>);
    rsx! {
        EditorDialog {
            title: if id.is_some() { "编辑模型服务" } else { "添加模型服务" },
            description: "名称自动生成。填写 URL 和 Key 后读取可用模型。",
            on_close: move |_| {
                let dialog = state.peek().settings_dialog();
                state.write().dialog = dialog;
            },
            on_saved: move |_| {
                let dialog = state.peek().settings_dialog();
                state.write().dialog = dialog;
            },
            save: move |_| {
                Box::pin(async move {
                    if selected().is_empty() {
                        return Err("请先读取并选择模型".into());
                    }
                    if loaded()
                        .as_ref()
                        .is_some_and(|(url, key)| url != &endpoint() || key != &secret())
                    {
                        return Err("地址或密钥已变化，请重新读取模型".into());
                    }
                    let draft = ProviderDraft {
                        label: String::new(),
                        endpoint: endpoint().trim().trim_end_matches('/').into(),
                        model: selected(),
                        secret: if secret().is_empty() { None } else { Some(secret()) },
                    };
                    let _: Provider = transport::request(
                            if id.is_some() { "PUT" } else { "POST" },
                            &id
                                .map(|id| format!("/providers/{id}"))
                                .unwrap_or_else(|| "/providers".into()),
                            json!(draft),
                        )
                        .await?;
                    state::load(state, false).await
                }) as az_ui_components::admin::AsyncResult<()>
            },
            label {
                "服务地址"
                Input {
                    aria_label: "服务地址",
                    value: endpoint,
                    placeholder: "https://example.com/v1",
                    oninput: move |e: FormEvent| {
                        endpoint.set(e.value());
                        models.clear();
                        selected.set(String::new());
                    },
                }
            }
            label {
                "API Key"
                Input {
                    r#type: "password",
                    aria_label: "API Key",
                    autocomplete: "new-password",
                    value: secret,
                    placeholder: if provider.as_ref().is_some_and(|p| p.has_secret) { "留空保留原密钥" } else { "输入 API Key" },
                    oninput: move |e: FormEvent| {
                        secret.set(e.value());
                        models.clear();
                    },
                }
            }
            Button {
                r#type: "button",
                variant: ButtonVariant::Outline,
                disabled: loading() ||
                        endpoint().trim().is_empty(),
                onclick: move |_| {
                    loading.set(true);
                    error.set(None);
                    let url = endpoint();
                    let key = secret();
                    spawn(async move {
                        let result: Result<Vec<String>, String> = transport::request(
                                "POST",
                                "/providers/models",
                                json!(
                                    ModelListRequest { provider_id : id, endpoint : url.trim()
                                    .trim_end_matches('/').into(), secret : if key.is_empty() { None
                                    } else { Some(key.clone()) } }
                                ),
                            )
                            .await;
                        loading.set(false);
                        if url != endpoint() || key != secret() {
                            return;
                        }
                        match result {
                            Ok(values) => {
                                if !values.contains(&selected()) {
                                    selected.set(values.first().cloned().unwrap_or_default());
                                }
                                models.set(values);
                                loaded.set(Some((url, key)));
                            }
                            Err(message) => error.set(Some(message)),
                        }
                    });
                },
                if loading() {
                    "读取中…"
                } else {
                    "读取模型"
                }
            }
            if !models().is_empty() {
                Select {
                    aria_label: "选择模型",
                    value: selected,
                    options: models().into_iter().map(| m |
                            SelectItem::new(m.clone(), m)).collect(),
                    on_value_change: move | value |
                            selected.set(value),
                }
            } else if !selected().is_empty() {
                p { "当前模型：{selected}" }
            }
            if let Some(error) = error() {
                p { role: "alert", "{error}" }
            }
        }
    }
}
#[component]
fn NewConversation() -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut title = use_signal(|| "新对话".to_owned());
    let mut provider = use_signal(|| {
        state
            .peek()
            .settings
            .as_ref()
            .and_then(|s| s.providers.first())
            .map(|p| p.id.to_string())
            .unwrap_or_default()
    });
    let mut space = use_signal(|| {
        state
            .peek()
            .spaces
            .iter()
            .find(|s| s.personal)
            .map(|s| s.id.clone())
            .unwrap_or_default()
    });
    let providers = state
        .read()
        .settings
        .as_ref()
        .map(|s| s.providers.clone())
        .unwrap_or_default();
    rsx! {
        EditorDialog {
            title: "新建对话",
            description: "模型可以在对话中随时切换。",
            on_close: move |_| state.write().dialog = None,
            on_saved: move | _ | state.write()
                    .dialog = None,
            save: move |_| {
                Box::pin(async move {
                    let provider_id = if provider().is_empty() {
                        None
                    } else {
                        Some(Uuid::parse_str(&provider()).map_err(|_| "模型选择无效")?)
                    };
                    let space_id = if space().is_empty() { None } else { Some(space()) };
                    let selected_space = state
                        .peek()
                        .spaces
                        .iter()
                        .find(|s| Some(&s.id) == space_id.as_ref())
                        .cloned();
                    if let Some(space) = selected_space && space.role == "OWNER"
                        && space.model_binding.is_none() && provider_id.is_some()
                    {
                        let _: Value = transport::memory(
                                "PUT",
                                &format!("/spaces/{}", space.id),
                                json!({ "title" : space.title, "modelBinding" : provider_id }),
                            )
                            .await?;
                    }
                    let created: Conversation = transport::request(
                            "POST",
                            "/conversations",
                            json!(ConversationDraft { provider_id, title : title(), space_id }),
                        )
                        .await?;
                    state::select(state, created.id).await?;
                    state::load(state, true).await
                }) as az_ui_components::admin::AsyncResult<()>
            },
            label {
                "标题"
                Input {
                    aria_label: "对话标题",
                    value: title,
                    oninput: move | e : FormEvent |
                            title.set(e.value()),
                }
            }
            Select {
                aria_label: "新对话模型",
                value: provider,
                options: std::iter::once(SelectItem::new("", "跟随空间模型"))
                    .chain(providers.iter().map(|p| SelectItem::new(p.id.to_string(), &p.model)))
                    .collect(),
                on_value_change: move |v| provider.set(v),
            }
            Select {
                aria_label: "记忆空间",
                value: space,
                options: std::iter::once(SelectItem::new("", "个人记忆空间"))
                    .chain(state.read().spaces.iter().map(|s| SelectItem::new(&s.id, &s.title)))
                    .collect(),
                on_value_change: move |v| space.set(v),
            }
        }
    }
}
