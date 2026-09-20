use crate::{
    state::{self, AgentState},
    transport,
};
use az_agent_model::Message;
use az_ui_components::{
    button::{Button, ButtonSize, ButtonVariant},
    markdown::Markdown,
};
use dioxus::prelude::*;
use dioxus_icons::lucide::{Check, Copy};
#[component]
pub(super) fn MessageView(message: Message) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let id = message.id;
    let mut copied = use_signal(|| false);
    let copy_content = message.content.clone();
    rsx! {
        article {
            class: "dx-conversation__message",
            "data-role": message.role.clone(),
            if !status(&message).is_empty() {
                header { span { role: "status", "{status(&message)}" } }
            }
            Markdown {
                source: message
                        .content.clone(),
                link_base: "https://aio.invalid/",
                image_base: "https://aio.invalid/",
                references: message
                    .citations
                    .iter()
                    .map(|c| (format!("memory:{}", c.id), c.title.clone()))
                    .collect(),
                on_reference: move |uri: String| {
                    if let Some(id) = uri.strip_prefix("memory:") {
                        let id = id.to_owned();
                        state::run(state, async move { state::open_entry(state, id).await });
                    }
                },
            }
            if let Some(error) = message.error.clone() {
                p { role: "alert", "{error}" }
            }
            div { class: "dx-conversation__references",
                if message.role == "assistant" && !message.content.is_empty() {
                    Button {
                        size: ButtonSize::IconSm, variant: ButtonVariant::Ghost,
                        aria_label: if copied() { "已复制回复" } else { "复制回复" },
                        title: if copied() { "已复制" } else { "复制回复" },
                        onclick: move |_| {
                            let content = copy_content.clone();
                            spawn(async move {
                                match transport::copy(content).await {
                                    Ok(()) => {
                                        copied.set(true);
                                        gloo_timers::future::TimeoutFuture::new(2000).await;
                                        copied.set(false);
                                    }
                                    Err(error) => state.write().error = Some(error),
                                }
                            });
                        },
                        if copied() { Check { size: 15 } } else { Copy { size: 15 } }
                    }
                }
                if let Some(source) = message.source_id.clone() {
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| {
                            let source = source.clone();
                            state::run(state, async move { state::open_source(state, source).await });
                        },
                        "来源资料"
                    }
                }
                for citation in message.citations {
                    Button {
                        key: "{citation.id}",
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: {
                            let id = citation.id.clone();
                            move |_| {
                                let id = id.clone();
                                state::run(state, async move { state::open_entry(state, id).await });
                            }
                        },
                        "{citation.title}"
                    }
                }
                if !message.activated_node_ids.is_empty() {
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| {
                            state.write().focused = Some(id);
                            state.write().show_graph = true;
                        },
                        "查看关联"
                    }
                }
                if let Some(tokens) = message.tokens {
                    small { "{tokens} tokens" }
                }
            }
        }
    }
}
fn status(message: &Message) -> &str {
    match message.status.as_str() {
        "generating" => "回复中",
        "awaiting_input" => "等待回答",
        "queued" => "整理中",
        "cancelled" => "已停止",
        "failed" => "失败",
        "interrupted" => "已中断",
        _ => {
            if message.memory_status.as_deref() == Some("complete") {
                "已记住"
            } else {
                ""
            }
        }
    }
}
