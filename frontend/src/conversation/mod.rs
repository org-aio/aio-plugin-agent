mod composer;
mod environment;
mod message;
mod sidebar;

use crate::state::{AgentState, Dialog};
use az_ui_components::button::{Button, ButtonSize, ButtonVariant};
use dioxus::prelude::*;
use dioxus_icons::lucide::{Network, PanelLeft, PanelRight, SquarePen};

/// 会话工作区入口：业务状态由 AgentState 持有，布局由共享组件库负责。
#[component]
pub fn ConversationPage() -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut collapsed = use_signal(|| false);
    let mut environment_open = use_signal(|| {
        web_sys::window()
            .and_then(|window| window.inner_width().ok())
            .and_then(|width| width.as_f64())
            .is_some_and(|width| width >= 1700.0)
    });
    crate::models::use_model_catalog();
    let mut scroll_listener = use_signal(|| None::<document::Eval>);
    use_drop(move || {
        if let Some(listener) = scroll_listener() {
            let _ = listener.send(());
        }
    });
    let view = state.read().clone();
    let empty = view.thread.as_ref().is_none_or(|t| t.messages.is_empty());
    let title = view
        .thread
        .as_ref()
        .map(|t| t.conversation.title.clone())
        .unwrap_or_else(|| "新对话".into());
    let toggle_sidebar = move |_| {
        collapsed.toggle();
        let next = !state.peek().show_history;
        state.write().show_history = next;
    };
    rsx! {
        main {
            class: "dx-conversation", "data-appearance": "codex",
            "data-history": view.show_history, "data-context": view.show_graph,
            "data-sidebar-collapsed": collapsed(),
            onkeydown: move |event: KeyboardEvent| {
                if event.key() == Key::Escape { state.write().show_history = false; environment_open.set(false); }
            },
            sidebar::Sidebar { on_toggle: toggle_sidebar }
            if view.show_history {
                button { class: "dx-conversation__scrim", aria_label: "关闭会话列表", onclick: move |_| state.write().show_history = false }
            }
            section { class: "dx-conversation__main", aria_label: "对话", "data-empty": empty,
                header { class: "dx-conversation__header",
                    Button { class: "dx-conversation__sidebar-toggle", size: ButtonSize::IconSm, variant: ButtonVariant::Ghost,
                        aria_label: "切换会话列表", title: "切换侧栏", onclick: toggle_sidebar,
                        PanelLeft { size: 18 }
                    }
                    span { class: "dx-conversation__title", "{title}" }
                    div { class: "dx-conversation__toolbar",
                        Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost,
                            aria_label: "环境信息", title: "环境信息", aria_pressed: environment_open(),
                            onclick: move |_| environment_open.toggle(), PanelRight { size: 18 }
                        }
                        if view.settings.as_ref().is_some_and(|s| s.memory_available) {
                            Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost,
                                aria_label: "知识图谱", title: "知识图谱", aria_pressed: view.show_graph,
                                onclick: move |_| { let next = !state.peek().show_graph; state.write().show_graph = next; },
                                Network { size: 18 }
                            }
                        }
                        Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost,
                            aria_label: "新对话", title: "新对话", disabled: view.busy,
                            onclick: move |_| state.write().dialog = Some(Dialog::New),
                            SquarePen { size: 18 }
                        }
                    }
                }
                if let Some(error) = view.error.clone() {
                    p { class: "dx-conversation__error", role: "alert", "{error}" }
                }
                div {
                    class: "dx-conversation__messages", aria_live: "polite",
                    onmounted: move |_| {
                        // 仅在用户仍靠近底部时跟随新消息，切换会话后重置滚动。
                        let listener = document::eval(r#"
                            const viewport = document.querySelector('.dx-conversation__messages');
                            let follow = true;
                            let thread = viewport.dataset.thread;
                            const scroll = () => { follow = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight < 80; };
                            const observer = new MutationObserver(() => {
                                if (thread !== viewport.dataset.thread) { thread = viewport.dataset.thread; follow = true; }
                                if (follow) viewport.scrollTop = viewport.scrollHeight;
                            });
                            observer.observe(viewport, {subtree:true, childList:true, characterData:true, attributes:true, attributeFilter:['data-thread']});
                            viewport.addEventListener('scroll', scroll, {passive:true});
                            viewport.scrollTop = viewport.scrollHeight;
                            await dioxus.recv(); observer.disconnect(); viewport.removeEventListener('scroll', scroll);
                        "#);
                        scroll_listener.set(Some(listener));
                    },
                    "data-thread": view.thread.as_ref().map(|t| t.conversation.id.to_string()).unwrap_or_default(),
                    if empty {
                        div { class: "dx-conversation__empty",
                            h1 { "今天想完成什么？" }
                            if view.settings.as_ref().is_some_and(|s| s.providers.is_empty()) {
                                Button { variant: ButtonVariant::Outline,
                                    onclick: move |_| state.write().dialog = Some(Dialog::Provider(None)), "配置模型服务"
                                }
                            }
                        }
                    }
                    if let Some(thread) = view.thread.as_ref() {
                        for message in &thread.messages {
                            message::MessageView { key: "{message.id}", message: message.clone() }
                        }
                        if let Some(request) = thread.pending_input.clone() {
                            div { class: "dx-conversation__pending",
                                crate::user_input::InputCard { key: "{request.id}", request }
                            }
                        }
                    }
                }
                composer::Composer {}

            }
            if environment_open() && !view.show_graph {
                environment::EnvironmentPanel { on_close: move |_| environment_open.set(false) }
            }
            if view.show_graph { crate::graph::GraphPanel {} }
        }
    }
}
