use crate::{
    state::{self, AgentState},
    transport,
};
use az_agent_model::{Conversation, ModelListRequest, ModelSelection};
use az_ui_components::{
    button::{Button, ButtonSize, ButtonVariant},
    select::{Select, SelectItem},
};
use dioxus::prelude::*;
use serde_json::json;
use uuid::Uuid;
#[component]
pub fn ConversationModel() -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let providers = state
        .read()
        .settings
        .as_ref()
        .map(|settings| settings.providers.clone())
        .unwrap_or_default();
    let mut catalog = use_resource(use_reactive!(|providers| async move {
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
    let current = state
        .read()
        .thread
        .as_ref()
        .map(|thread| thread.conversation.clone());
    let selected = current
        .as_ref()
        .and_then(|conversation| {
            conversation.provider_id.map(|id| {
                let model = conversation
                    .model
                    .clone()
                    .or_else(|| {
                        providers
                            .iter()
                            .find(|p| p.id == id)
                            .map(|p| p.model.clone())
                    })
                    .unwrap_or_default();
                json!([id, model]).to_string()
            })
        })
        .unwrap_or_default();
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
                value: selected,
                options,
                disabled: state.read().busy || state
                        .read().processing(),
                on_value_change: move |value: String| {
                    let id = state.peek().thread.as_ref().map(|thread| thread.conversation.id);
                    if let Some(id) = id {
                        state::run(
                            state,
                            async move {
                                let selection = if value.is_empty() {
                                    ModelSelection {
                                        provider_id: None,
                                        model: None,
                                    }
                                } else {
                                    let (provider, model): (Uuid, String) = serde_json::from_str(
                                            &value,
                                        )
                                        .map_err(|_| "模型选择无效")?;
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
                            },
                        );
                    }
                },
            }
            Button {
                size: ButtonSize::Sm,
                variant: ButtonVariant::Ghost,
                aria_label: "刷新模型列表",
                disabled: ! catalog
                        .finished(),
                onclick: move |_| catalog.restart(),
                "↻"
            }
            if let Some((_, failures)) = catalog.read().as_ref() {
                if !failures.is_empty() {
                    small { role: "status", title: failures.join("；"), "部分服务模型列表不可用" }
                }
            }
        }
    }
}
