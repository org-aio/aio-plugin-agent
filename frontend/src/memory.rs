use crate::{
    state::{self, AgentState},
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
#[component]
pub fn SourceDialog(source: MemorySource) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut review = use_signal(|| false);
    rsx! {
        Dialog {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    state.write().dialog = None;
                }
            },
            DialogTitle { "来源资料" }
            p { class: "dx-conversation__source", "{source.text}" }
            if let Some(error) = source.error {
                p { role: "alert", "{error}" }
            }
            for secret in source.secrets {
                SecretField { key: "{secret.id}", secret }
            }
            if source.status == "conflict" {
                Button { onclick: move |_| review.set(true), "核实修订" }
            }
            if source.status == "failed" {
                Button {
                    onclick: {
                        let id = source.id.clone();
                        move |_| {
                            let id = id.clone();
                            state::run(
                                state,
                                async move {
                                    let _: Value = transport::memory(
                                            "POST",
                                            &format!("/sources/{id}/retry"),
                                            Value::Null,
                                        )
                                        .await?;
                                    state::open_source(state, id).await
                                },
                            );
                        }
                    },
                    "重新整理"
                }
            }
            Button {
                variant: ButtonVariant::Outline,
                onclick: move |_| state.write().dialog = None,
                "关闭"
            }
            if review() {
                ReviewDialog { id: source.id, on_close: move | _
                            | review.set(false) }
            }
        }
    }
}
#[component]
fn SecretField(secret: SecretReference) -> Element {
    let mut value = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut grant = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut epoch = use_signal(|| 0u64);
    rsx! {
        section {
            h3 { "{secret.label}" }
            p { class: "dx-conversation__source", "{value().as_deref().unwrap_or(\"••••••••\")}" }
            div { class: "flex gap-2",
                Button {
                    variant: ButtonVariant::Outline,
                    disabled: ! secret.can_reveal ||
                            busy(),
                    onclick: {
                        let id = secret.id.clone();
                        move |_| {
                            if value().is_some() {
                                value.set(None);
                                epoch += 1;
                                return;
                            }
                            busy.set(true);
                            let id = id.clone();
                            spawn(async move {
                                match transport::memory::<
                                    Value,
                                >("POST", &format!("/secrets/{id}/reveal"), Value::Null)
                                    .await
                                {
                                    Ok(result) => {
                                        value.set(result["value"].as_str().map(str::to_owned));
                                        epoch += 1;
                                        let current = epoch();
                                        spawn(async move {
                                            gloo_timers::future::TimeoutFuture::new(30000).await;
                                            if epoch() == current {
                                                value.set(None);
                                            }
                                        });
                                    }
                                    Err(message) => error.set(Some(message)),
                                }
                                busy.set(false);
                            });
                        }
                    },
                    if value().is_some() {
                        "隐藏秘密"
                    } else {
                        "查看秘密"
                    }
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    disabled: value().is_none(),
                    onclick: move |_| {
                        if let Some(value) = value() {
                            spawn(async move {
                                if let Err(message) = transport::copy(value).await {
                                    error.set(Some(message));
                                }
                            });
                        }
                    },
                    "复制"
                }
                if secret.can_manage {
                    Button {
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| grant.set(true),
                        "秘密授权"
                    }
                }
            }
            if let Some(error) = error() {
                p { role: "alert", "{error}" }
            }
            if grant() {
                GrantDialog { id: secret.id, on_close: move |_| grant.set(false) }
            }
        }
    }
}
#[component]
fn GrantDialog(id: String, on_close: Callback<()>) -> Element {
    let mut user = use_signal(String::new);
    let mut reveal = use_signal(|| true);
    let mut manage = use_signal(|| false);
    rsx! {
        EditorDialog {
            title: "秘密授权",
            description: "为记忆空间中的成员授予权限。",
            on_close,
            on_saved: move |_| on_close.call(()),
            save: move |_| {
                let id = id.clone();
                Box::pin(async move {
                    let _: Value = transport::memory(
                            "PUT",
                            &format!("/secrets/{id}/grants"),
                            json!(
                                { "userId" : user().trim(), "reveal" : reveal(), "manage" :
                                manage() }
                            ),
                        )
                        .await?;
                    Ok(())
                }) as az_ui_components::admin::AsyncResult<()>
            },
            label {
                "空间成员 ID"
                Input {
                    aria_label: "空间成员 ID",
                    value: user,
                    oninput: move | e : FormEvent |
                            user.set(e.value()),
                }
            }
            label {
                Checkbox {
                    checked: Some(az_ui_components::checkbox::checkbox_state(reveal())),
                    on_checked_change: move |v| reveal.set(bool::from(v)),
                }
                "允许查看与复制"
            }
            label {
                Checkbox {
                    checked: Some(az_ui_components::checkbox::checkbox_state(manage())),
                    on_checked_change: move |v| manage.set(bool::from(v)),
                }
                "允许管理授权"
            }
        }
    }
}
#[component]
pub fn EntryDialog(id: String, title: String, content: String) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let sources = use_resource(move || {
        let id = id.clone();
        async move {
            transport::memory::<Vec<MemorySource>>(
                "GET",
                &format!("/nodes/{id}/sources"),
                Value::Null,
            )
            .await
        }
    });
    rsx! {
        Dialog {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    state.write().dialog = None;
                }
            },
            DialogTitle { "{title}" }
            p { class: "dx-conversation__source", "{content}" }
            if let Some(result) = sources.read().as_ref() {
                match result {
                    Ok(sources) => rsx! {
                        for source in sources {
                            Button {
                                key: "{source.id}",
                                variant: ButtonVariant::Ghost,
                                onclick: {
                                    let id = source.id.clone();
                                    move |_| {
                                        let id = id.clone();
                                        state::run(state, async move { state::open_source(state, id).await });
                                    }
                                },
                                "{source.text.lines().next().unwrap_or(\"来源资料\")}"
                            }
                        }
                    },
                    Err(error) => rsx! {
                        p { role: "alert", "{error}" }
                    },
                }
            }
            Button {
                variant: ButtonVariant::Outline,
                onclick: move |_| state.write().dialog = None,
                "关闭"
            }
        }
    }
}
#[component]
pub fn SpacesDialog() -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut selected = use_signal(String::new);
    let mut title = use_signal(String::new);
    let mut model = use_signal(String::new);
    let mut members = use_signal(Vec::<Value>::new);
    let mut member = use_signal(String::new);
    let mut role = use_signal(|| "READER".to_owned());
    let mut remove = use_signal(|| None::<String>);
    let spaces = state.read().spaces.clone();
    let owner = selected().is_empty()
        || spaces
            .iter()
            .any(|s| s.id == selected() && s.role == "OWNER");
    let providers = state
        .read()
        .settings
        .as_ref()
        .map(|s| s.providers.clone())
        .unwrap_or_default();
    rsx! {
        EditorDialog {
            title: "记忆空间",
            description: "管理独立的记忆空间、整理模型和成员。",
            on_close: move |_| state.write().dialog = None,
            on_saved: move | _ | state.write()
                    .dialog = None,
            save: move |_| {
                Box::pin(async move {
                    if !owner {
                        return Err("只有空间所有者可以修改".into());
                    }
                    let id = selected();
                    let path = if id.is_empty() {
                        "/spaces".into()
                    } else {
                        format!("/spaces/{id}")
                    };
                    let _: Value = transport::memory(
                            if id.is_empty() { "POST" } else { "PUT" },
                            &path,
                            json!(
                                { "title" : title().trim(), "modelBinding" : if model()
                                .is_empty() { None } else { Some(model()) } }
                            ),
                        )
                        .await?;
                    state::load(state, false).await
                }) as az_ui_components::admin::AsyncResult<()>
            },
            Select {
                aria_label: "管理记忆空间",
                value: selected,
                options: std::iter::once(SelectItem::new("", "新建空间"))
                    .chain(spaces.iter().map(|s| SelectItem::new(&s.id, &s.title)))
                    .collect(),
                on_value_change: {
                    let spaces = spaces.clone();
                    move |id: String| {
                        selected.set(id.clone());
                        let space = spaces.iter().find(|s| s.id == id);
                        title.set(space.map(|s| s.title.clone()).unwrap_or_default());
                        model.set(space.and_then(|s| s.model_binding.clone()).unwrap_or_default());
                        members.clear();
                        if !id.is_empty() {
                            state::run(
                                state,
                                async move {
                                    members
                                        .set(
                                            transport::memory(
                                                    "GET",
                                                    &format!("/spaces/{id}/members"),
                                                    Value::Null,
                                                )
                                                .await?,
                                        );
                                    Ok(())
                                },
                            );
                        }
                    }
                },
            }
            label {
                "空间名称"
                Input {
                    aria_label: "空间名称",
                    value: title,
                    disabled: !owner,
                    oninput: move | e : FormEvent
                            | title.set(e.value()),
                }
            }
            Select {
                aria_label: "整理模型",
                value: model,
                disabled: !owner,
                options: std::iter::once(SelectItem::new("", "暂不整理"))
                    .chain(providers.iter().map(|p| SelectItem::new(p.id.to_string(), &p.model)))
                    .collect(),
                on_value_change: move |v| model.set(v),
            }
            if !selected().is_empty() {
                for item in members() {
                    div { class: "dx-conversation-settings__row",
                        span {
                            "{item[\"userId\"].as_str().unwrap_or_default()} · {item[\"role\"].as_str().unwrap_or_default()}"
                        }
                        if owner {
                            Button {
                                r#type: "button",
                                variant: ButtonVariant::Ghost,
                                onclick: move |_| remove.set(item["userId"].as_str().map(str::to_owned)),
                                "移除"
                            }
                        }
                    }
                }
                if owner {
                    Input {
                        aria_label: "添加成员 ID",
                        placeholder: "成员 ID",
                        value: member,
                        oninput: move | e : FormEvent
                                | member.set(e.value()),
                    }
                    Select {
                        aria_label: "成员角色",
                        value: role,
                        options: [
                            ("READER", "只读"),
                            ("EDITOR", "编辑"),
                            ("OWNER", "所有者"),
                        ]
                            .into_iter()
                            .map(|(v, l)| SelectItem::new(v, l))
                            .collect(),
                        on_value_change: move |v| role.set(v),
                    }
                    Button {
                        r#type: "button",
                        variant: ButtonVariant::Outline,
                        disabled: member().trim().is_empty(),
                        onclick: move |_| state::run(
                            state,
                            async move {
                                let _: Value = transport::memory(
                                        "POST",
                                        &format!("/spaces/{}/members", selected()),
                                        json!({ "userId" : member().trim(), "role" : role() }),
                                    )
                                    .await?;
                                members
                                    .set(
                                        transport::memory(
                                                "GET",
                                                &format!("/spaces/{}/members", selected()),
                                                Value::Null,
                                            )
                                            .await?,
                                    );
                                member.set(String::new());
                                Ok(())
                            },
                        ),
                        "添加成员"
                    }
                }
            }
            if let Some(id) = remove() {
                DeleteRecordsDialog {
                    items: vec![String::from("此项")],
                    item_label: move |value: String| value,
                    title: "移除成员",
                    warning: "移除后该成员将失去此空间权限。",
                    on_close: move |_| remove.set(None),
                    on_deleted: move | _ |
                            remove.set(None),
                    delete: move |_| {
                        let id = id.clone();
                        Box::pin(async move {
                            let _: Value = transport::memory(
                                    "DELETE",
                                    &format!("/spaces/{}/members/{id}", selected()),
                                    Value::Null,
                                )
                                .await?;
                            members
                                .set(
                                    transport::memory(
                                            "GET",
                                            &format!("/spaces/{}/members", selected()),
                                            Value::Null,
                                        )
                                        .await?,
                                );
                            Ok(())
                        }) as az_ui_components::admin::AsyncResult<()>
                    },
                }
            }
        }
    }
}
#[component]
fn ReviewDialog(id: String, on_close: Callback<()>) -> Element {
    let state = use_context::<Signal<AgentState>>();
    let request_id = id.clone();
    let proposal = use_resource(move || {
        let id = request_id.clone();
        async move {
            let proposal: Value =
                transport::memory("GET", &format!("/sources/{id}/proposal"), Value::Null).await?;
            let mut current = serde_json::Map::new();
            if let Some(entries) = proposal["entries"].as_array() {
                for entry in entries {
                    if let Some(id) = entry["existingId"].as_str() {
                        let node: Value =
                            transport::memory("GET", &format!("/nodes/{id}"), Value::Null).await?;
                        current.insert(id.into(), node);
                    }
                }
            }
            Ok::<_, String>((proposal, current))
        }
    });
    let resolve = move |accept: bool| {
        let id = id.clone();
        state::run(state, async move {
            let current = proposal
                .read()
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .map(|(_, current)| current.clone())
                .ok_or("修订尚未加载")?;
            let versions: serde_json::Map<String, Value> = current
                .into_iter()
                .map(|(id, node)| (id, node["version"].clone()))
                .collect();
            let _: Value = transport::memory(
                "POST",
                &format!("/sources/{id}/resolve"),
                json!({ "accept" : accept, "versions" : versions }),
            )
            .await?;
            on_close.call(());
            state::open_source(state, id).await
        });
    };
    let accept = resolve.clone();
    rsx! {
        Dialog {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    on_close.call(())
                }
            },
            DialogTitle { "核实修订" }
            if let Some(result) = proposal.read().as_ref() {
                match result {
                    Ok((proposal, current)) => rsx! {
                        if let Some(entries) = proposal["entries"].as_array() {
                            for entry in entries {
                                section {
                                    h3 { "{entry[\"draft\"][\"title\"].as_str().unwrap_or(\"修订\")}" }
                                    if let Some(node) = entry["existingId"].as_str().and_then(|id| current.get(id)) {
                                        p { "当前：{node[\"content\"].as_str().unwrap_or_default()}" }
                                    }
                                    p { "建议：{entry[\"draft\"][\"content\"].as_str().unwrap_or_default()}" }
                                }
                            }
                        }
                    },
                    Err(error) => rsx! {
                        p { role: "alert", "{error}" }
                    },
                }
            }
            Button {
                disabled: !proposal.finished() || state.read().busy,
                onclick: move | _ |
                        accept(true),
                "接受修订"
            }
            Button {
                variant: ButtonVariant::Outline,
                disabled: !proposal.finished() || state.read().busy,
                onclick: move | _ |
                        resolve(false),
                "保留现有内容"
            }
        }
    }
}
