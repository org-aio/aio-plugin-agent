use crate::{
    state::{self, AgentState},
    transport,
};
use az_agent_model::{Conversation, ModelListRequest, ModelSelection};
use az_ui_components::{
    button::{Button, ButtonSize, ButtonVariant},
    select::{Select, SelectItem, SelectPlacement},
};
use dioxus::prelude::*;
use dioxus_icons::lucide::RefreshCw;
use serde_json::json;
use uuid::Uuid;

type ModelCatalog = Resource<(Vec<SelectItem>, Vec<String>)>;

/// 在工作区共享模型目录，快捷条与下拉框刷新同一份数据。
pub fn use_model_catalog() {
    let state = use_context::<Signal<AgentState>>();
    let providers = state
        .read()
        .settings
        .as_ref()
        .map(|settings| settings.providers.clone())
        .unwrap_or_default();
    let catalog = use_resource(use_reactive!(|providers| async move {
        let mut options = Vec::new();
        let mut failures = Vec::new();
        for provider in providers {
            let result: Result<Vec<String>, String> = transport::request(
                "POST",
                "/providers/models",
                json!(ModelListRequest {
                    provider_id: Some(provider.id),
                    endpoint: provider.endpoint.clone(),
                    secret: None
                }),
            )
            .await;
            match result {
                Ok(models) => {
                    for model in models {
                        options.push(SelectItem::new(
                            json!([provider.id, model]).to_string(),
                            format!("{model} · {}", provider.label),
                        ));
                    }
                }
                Err(error) => {
                    options.push(SelectItem::new(
                        json!([provider.id, provider.model]).to_string(),
                        format!("{} · {}", provider.model, provider.label),
                    ));
                    failures.push(format!("{}：{error}", provider.label));
                }
            }
        }
        (options, failures)
    }));
    use_context_provider(|| catalog);
}

fn selected_model(view: &AgentState) -> String {
    let current = view.thread.as_ref().map(|thread| &thread.conversation);
    current
        .and_then(|conversation| {
            let id = conversation.provider_id?;
            let model = conversation
                .model
                .clone()
                .or_else(|| {
                    view.settings
                        .as_ref()?
                        .providers
                        .iter()
                        .find(|p| p.id == id)
                        .map(|p| p.model.clone())
                })
                .unwrap_or_default();
            Some(json!([id, model]).to_string())
        })
        .unwrap_or_default()
}

/// 快捷条与下拉框共用真实的会话模型写入，成功后采用服务端返回值。
fn select_model(mut state: Signal<AgentState>, value: String) {
    let id = state
        .peek()
        .thread
        .as_ref()
        .map(|thread| thread.conversation.id);
    let Some(id) = id else {
        return;
    };
    state::run(state, async move {
        let selection = if value.is_empty() {
            ModelSelection {
                provider_id: None,
                model: None,
            }
        } else {
            let (provider, model): (Uuid, String) =
                serde_json::from_str(&value).map_err(|_| "模型选择无效")?;
            ModelSelection {
                provider_id: Some(provider),
                model: Some(model),
            }
        };
        let conversation: Conversation = transport::request(
            "PUT",
            &format!("/conversations/{id}/model"),
            json!(selection),
        )
        .await?;
        if let Some(thread) = state.write().thread.as_mut() {
            thread.conversation = conversation;
        }
        Ok(())
    });
}

#[component]
pub fn ConversationModel() -> Element {
    let state = use_context::<Signal<AgentState>>();
    let catalog = use_context::<ModelCatalog>();
    let view = state.read().clone();
    let current = view.thread.as_ref().map(|thread| &thread.conversation);
    let selected = selected_model(&view);
    let mut options = vec![SelectItem::new("", "跟随空间模型")];
    if let Some((loaded, _)) = catalog.read().as_ref() {
        options.extend(loaded.clone());
    }
    if !selected.is_empty() && !options.iter().any(|option| option.value == selected) {
        options.push(SelectItem::new(
            &selected,
            current
                .as_ref()
                .and_then(|c| c.model.as_deref())
                .unwrap_or("当前模型"),
        ));
    }
    rsx! {
        div { class: "dx-conversation__model-selector",
            Select {
                aria_label: "对话模型",
                placement: SelectPlacement::Top,
                value: selected,
                options,
                disabled: view.busy || view.processing() || view.thread.as_ref().is_some_and(|t| t.pending_input.is_some()),
                on_value_change: move |value: String| select_model(state, value),
            }
            if let Some((_, failures)) = catalog.read().as_ref() {
                if !failures.is_empty() {
                    small { role: "status", title: failures.join("；"), "部分服务模型列表不可用" }
                }
            }
        }
    }
}

/// 展示真实可选模型；完整目录保留在下拉框中，不冒充持久化收藏。
#[component]
pub fn ModelShortcuts() -> Element {
    let state = use_context::<Signal<AgentState>>();
    let mut catalog = use_context::<ModelCatalog>();
    let view = state.read().clone();
    let selected = selected_model(&view);
    let disabled = view.busy
        || view.processing()
        || view
            .thread
            .as_ref()
            .is_some_and(|t| t.pending_input.is_some());
    let models = catalog
        .read()
        .as_ref()
        .map(|(models, _)| models.clone())
        .unwrap_or_default();
    rsx! {
        div { class: "dx-conversation__model-shortcuts", aria_label: "模型快捷选择",
            span { class: "dx-conversation__shortcut-label", "可用模型" }
            for option in models.iter().take(8) {
                Button { key: "{option.value}", r#type: "button", variant: ButtonVariant::Ghost,
                    title: option.label.clone(), aria_label: format!("切换模型 {}", option.label),
                    aria_pressed: option.value == selected, disabled,
                    onclick: { let value = option.value.clone(); move |_| select_model(state, value.clone()) },
                    {option.label.split(" · ").next().unwrap_or(&option.label)}
                }
            }
            Button { r#type: "button", size: ButtonSize::IconSm, variant: ButtonVariant::Ghost,
                aria_label: "刷新模型列表", title: "刷新模型", disabled: !catalog.finished(),
                onclick: move |_| catalog.restart(), RefreshCw { size: 14 }
            }
        }
    }
}
