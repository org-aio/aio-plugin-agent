use crate::{
    state::{self, AgentState, Dialog},
    transport,
};
use az_agent_model::*;
use az_ui_components::{
    button::{Button, ButtonSize, ButtonVariant},
    input::Input,
    markdown::Markdown,
    textarea::Textarea,
};
use dioxus::prelude::*;
use serde_json::Value;
#[component]
pub fn ConversationPage() -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut scroll_listener = use_signal(|| None::<document::Eval>);
    use_drop(move || {
        if let Some(listener) = scroll_listener() {
            let _ = listener.send(());
        }
    });
    let view = state.read().clone();
    let settings = view.settings.as_ref();
    let thread = view.thread.clone();
    let providers = settings.map(|s| s.providers.clone()).unwrap_or_default();
    rsx! {
        main {
            class: "dx-conversation",
            "data-history": view.show_history,
            "data-context": view.show_graph,
            header { class: "dx-conversation__header",
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Ghost,
                    onclick: move |_| {
                        let next = !state.peek().show_history;
                        state.write().show_history = next;
                    },
                    "会话"
                }
                strong { "智能体" }
                div { class: "dx-conversation__toolbar",
                    if settings.is_some_and(|s| s.memory_available) {
                        Button {
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Ghost,
                            onclick: move |
                                    _ | state.write().dialog = Some(Dialog::Spaces),
                            "记忆空间"
                        }
                        Button {
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Ghost,
                            onclick: move |_| {
                                let next = !state.peek().show_graph;
                                state.write().show_graph = next;
                            },
                            "知识图谱"
                        }
                    }
                    Button {
                        size: ButtonSize::Sm,
                        variant: ButtonVariant::Ghost,
                        onclick: move |
                                _ | state.write().dialog = Some(Dialog::Settings),
                        "设置"
                    }
                    Button {
                        size: ButtonSize::Sm,
                        onclick: move |_| state.write().dialog = Some(Dialog::New),
                        "新对话"
                    }
                }
            }
            aside { class: "dx-conversation__history", aria_label: "会话列表",
                Input {
                    aria_label: "搜索会话",
                    placeholder: "搜索会话",
                    value: view.search.clone(),
                    oninput: move | e : FormEvent |
                            state.write().search = e.value(),
                }
                nav {
                    for item in view.conversations.iter().filter(|c| c.title.contains(&view.search)) {
                        div {
                            key: "{item.id}",
                            class: "dx-conversation__history-row",
                            "data-selected": thread.as_ref().is_some_and(| t
                                    | t.conversation.id == item.id),
                            Button {
                                variant: ButtonVariant::Ghost,
                                disabled: view.busy,
                                onclick: {
                                    let id = item.id;
                                    move |_| state::run(state, async move { state::select(state, id).await })
                                },
                                span { "{item.title}" }
                            }
                            Button {
                                size: ButtonSize::Sm,
                                variant: ButtonVariant::Ghost,
                                aria_label: format!("删除会话 {}", item.title),
                                onclick: {
                                    let title = item.title.clone();
                                    let path = format!("/conversations/{}", item.id);
                                    move |_| {
                                        state.write().dialog = Some(Dialog::Delete {
                                            title: title.clone(),
                                            path: path.clone(),
                                        });
                                    }
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            section { class: "dx-conversation__main", aria_label: "对话",
                div { class: "dx-conversation__model",
                    span {
                        "{thread.as_ref().map(|t|t.conversation.title.as_str()).unwrap_or(\"新对话\")}"
                    }
                    crate::models::ConversationModel {}
                }
                if let Some(error) = view.error.clone() {
                    p { class: "dx-conversation__error", role: "alert", "{error}" }
                }
                div {
                    class: "dx-conversation__messages",
                    aria_live: "polite",
                    onmounted: move |_| {
                        let listener = document::eval(
                            r#"
                                                        const viewport=document.querySelector('.dx-conversation__messages');
                                                        let follow=true;
                                                        const scroll=()=>{follow=viewport.scrollHeight-viewport.scrollTop-viewport.clientHeight<80;};
                                                        const observer=new MutationObserver(()=>{if(follow)viewport.scrollTop=viewport.scrollHeight;});
                                                        observer.observe(viewport,{subtree:true,childList:true,characterData:true});
                                                        viewport.addEventListener('scroll',scroll,{passive:true});
                                                        viewport.scrollTop=viewport.scrollHeight;
                                                        await dioxus.recv();observer.disconnect();viewport.removeEventListener('scroll',scroll);
                                                    "#,
                        );
                        scroll_listener.set(Some(listener));
                    },
                    if let Some(thread) = thread {
                        if thread.messages.is_empty() {
                            div { class: "dx-conversation__empty",
                                h2 { "有什么想聊的？" }
                                p { "提问，或把需要记住的资料发给我。" }
                                if providers.is_empty() {
                                    Button {
                                        variant: ButtonVariant::Outline,
                                        onclick: move | _ | state.write().dialog =
                                                Some(Dialog::Provider(None)),
                                        "配置模型服务"
                                    }
                                }
                            }
                        }
                        for message in thread.messages {
                            MessageView { key: "{message.id}", message }
                        }
                    }
                }
                form {
                    class: "dx-conversation__composer",
                    onsubmit: move |e: FormEvent| {
                        e.prevent_default();
                        state::run(state, async move { state::send(state).await });
                    },
                    Textarea {
                        aria_label: "发送消息",
                        placeholder: "发送消息…",
                        rows: "3",
                        value: view.draft.clone(),
                        disabled: view.busy || view.running(),
                        oninput: move |e: FormEvent| state.write().draft = e.value(),
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter && !e.modifiers().shift() {
                                e.prevent_default();
                                if !state.peek().running() {
                                    state::run(state, async move { state::send(state).await });
                                }
                            }
                        },
                    }
                    div { class: "dx-conversation__composer-actions",
                        small {
                            if view.running() {
                                "正在回复"
                            } else {
                                "Enter 发送 · Shift+Enter 换行"
                            }
                        }
                        Button {
                            r#type: "button",
                            variant: if view.running() { ButtonVariant::Outline } else { ButtonVariant::Primary },
                            disabled: view.busy || (! view.running() && view
                                    .draft.trim().is_empty()),
                            onclick: move |_| {
                                if state.peek().running() {
                                    let id = state.peek().thread.as_ref().map(|t| t.conversation.id);
                                    if let Some(id) = id {
                                        state::run(
                                            state,
                                            async move {
                                                let thread = transport::request(
                                                        "POST",
                                                        &format!("/conversations/{id}/cancel"),
                                                        Value::Null,
                                                    )
                                                    .await?;
                                                state.write().thread = Some(thread);
                                                Ok(())
                                            },
                                        );
                                    }
                                } else {
                                    state::run(state, async move { state::send(state).await });
                                }
                            },
                            if view.running() {
                                "停止"
                            } else {
                                "发送"
                            }
                        }
                    }
                }
            }
            if view.show_graph {
                crate::graph::GraphPanel {}
            }
        }
    }
}
#[component]
fn MessageView(message: Message) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let id = message.id;
    rsx! {
        article {
            class: "dx-conversation__message",
            "data-role": message.role.clone(),
            header {
                strong {
                    if message.role == "user" {
                        "你"
                    } else {
                        "智能体"
                    }
                }
                span { "{status(&message)}" }
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
