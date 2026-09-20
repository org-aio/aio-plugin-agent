use crate::{
    state::{self, AgentState, Dialog},
    transport,
};
use az_ui_components::{
    button::{Button, ButtonSize, ButtonVariant},
    textarea::Textarea,
};
use dioxus::prelude::*;
use dioxus_icons::lucide::{ArrowUp, Brain, Plus, Settings, Square};
use serde_json::Value;

/// 输入区持有输入法状态；模型、设备选择仍保存到当前会话。
#[component]
pub(super) fn Composer() -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut composing = use_signal(|| false);
    let mut more_open = use_signal(|| false);
    let view = state.read().clone();
    let awaiting = view
        .thread
        .as_ref()
        .is_some_and(|t| t.pending_input.is_some());
    let disabled = view.busy || view.processing() || awaiting;
    rsx! {
        div { class: "dx-conversation__composer-wrap",
            onkeydown: move |event: KeyboardEvent| {
                if event.key() == Key::Escape { more_open.set(false); }
            },
            form { class: "dx-conversation__composer",
                onsubmit: move |e: FormEvent| {
                    e.prevent_default();
                    if !disabled { state::run(state, async move { state::send(state).await }); }
                },
                Textarea {
                    aria_label: "发送消息", placeholder: "向智能体发送消息…", rows: "2",
                    value: view.draft.clone(), disabled,
                    oncompositionstart: move |_| composing.set(true),
                    oncompositionend: move |_| composing.set(false),
                    oninput: move |e: FormEvent| state.write().draft = e.value(),
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Enter && !e.modifiers().shift() && !composing() && !e.is_composing() {
                            e.prevent_default();
                            if !disabled { state::run(state, async move { state::send(state).await }); }
                        }
                    },
                }
                div { class: "dx-conversation__composer-actions",
                    div { class: "dx-conversation__more",
                        button { r#type: "button", aria_label: "更多选项", title: "更多选项",
                            aria_expanded: more_open(), aria_controls: "conversation-actions",
                            onclick: move |_| more_open.toggle(), Plus { size: 20 }
                        }
                        if more_open() {
                            button { r#type: "button", class: "dx-conversation__menu-dismiss", aria_label: "关闭更多选项", onclick: move |_| more_open.set(false) }
                            div { id: "conversation-actions", class: "dx-conversation__action-menu",
                            if let Some(current) = view.thread.as_ref() { crate::swarm::SwarmTasks { conversation: current.conversation.id } }
                            if view.settings.as_ref().is_some_and(|s| s.memory_available) {
                                Button { r#type: "button", variant: ButtonVariant::Ghost,
                                    onclick: move |_| { more_open.set(false); state.write().dialog = Some(Dialog::Spaces); },
                                    Brain { size: 16 } "记忆空间"
                                }
                            }
                            Button { r#type: "button", variant: ButtonVariant::Ghost,
                                onclick: move |_| { more_open.set(false); state.write().dialog = Some(Dialog::Settings); },
                                Settings { size: 16 } "模型与工具设置"
                            }
                            }
                        }
                    }
                    crate::models::ConversationModel {}
                    Button { class: "dx-conversation__send", r#type: "button", size: ButtonSize::Icon,
                        aria_label: if view.running() { "停止" } else { "发送" },
                        title: if view.running() { "停止生成" } else { "发送消息" },
                        disabled: view.busy || awaiting || (!view.running() && (view.processing() || view.draft.trim().is_empty())),
                        onclick: move |_| {
                            if state.peek().running() {
                                let id = state.peek().thread.as_ref().map(|t| t.conversation.id);
                                if let Some(id) = id {
                                    state::run(state, async move {
                                        let thread = transport::request("POST", &format!("/conversations/{id}/cancel"), Value::Null).await?;
                                        state.write().thread = Some(thread);
                                        Ok(())
                                    });
                                }
                            } else {
                                state::run(state, async move { state::send(state).await });
                            }
                        },
                        if view.running() { Square { size: 14, fill: "currentColor" } } else { ArrowUp { size: 19 } }
                    }
                }
            }
            div { class: "dx-conversation__composer-footer",
                crate::user_input::ConversationDevice {}
                small { role: "status",
                    if awaiting { "等待回答后继续" }
                    else if view.processing() { "正在处理…" }
                    else { "Shift + Enter 换行" }
                }
            }
        }
    }
}
