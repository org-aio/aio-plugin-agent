use crate::state::{self, AgentState, Dialog};
use az_agent_model::Conversation;
use az_ui_components::{
    button::{Button, ButtonSize, ButtonVariant},
    input::Input,
};
use dioxus::prelude::*;
use dioxus_icons::lucide::{
    Brain, ChevronDown, Folder, PanelLeft, Search, Settings, SquarePen, Trash2,
};

/// 侧栏只展示真实会话与已开放能力，菜单操作继续走现有共享 Dialog。
#[component]
pub(super) fn Sidebar(on_toggle: EventHandler<MouseEvent>) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut searching = use_signal(|| false);
    let view = state.read().clone();
    let query = view.search.to_lowercase();
    let conversations: Vec<_> = view
        .conversations
        .iter()
        .filter(|c| c.title.to_lowercase().contains(&query))
        .collect();
    rsx! {
        aside { class: "dx-conversation__history", aria_label: "会话列表",
            div { class: "dx-conversation__sidebar-heading",
                span { class: "dx-conversation__brand", "AIO" }
                Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost,
                    title: "收起侧栏", aria_label: "收起会话列表", onclick: on_toggle,
                    PanelLeft { size: 18 }
                }
            }
            div { class: "dx-conversation__navigation",
                Button { variant: ButtonVariant::Ghost, disabled: view.busy,
                    onclick: move |_| state.write().dialog = Some(Dialog::New),
                    SquarePen { size: 18 } span { "新对话" }
                }
                Button { variant: ButtonVariant::Ghost, aria_expanded: searching(),
                    onclick: move |_| { searching.toggle(); if !searching() { state.write().search.clear(); } },
                    Search { size: 18 } span { "搜索对话" }
                }
                if view.settings.as_ref().is_some_and(|s| s.memory_available) {
                    Button { variant: ButtonVariant::Ghost, onclick: move |_| state.write().dialog = Some(Dialog::Spaces),
                        Brain { size: 18 } span { "记忆空间" }
                    }
                }
            }
            if searching() {
                Input { aria_label: "搜索会话", placeholder: "搜索会话", onmounted: move |event: MountedEvent| { spawn(async move { let _ = event.set_focus(true).await; }); },
                    value: view.search.clone(), oninput: move |e: FormEvent| state.write().search = e.value()
                }
            }
            p { class: "dx-conversation__section-label", "空间与对话" }
            nav { aria_label: "历史会话",
                if conversations.is_empty() { p { class: "dx-conversation__no-results", "没有匹配的对话" } }
                for space in &view.spaces {
                    if conversations.iter().any(|c| c.space_id.as_ref() == Some(&space.id)) {
                        details { key: "{space.id}", class: "dx-conversation__history-group", open: true,
                            summary { ChevronDown { size: 12 } Folder { size: 16 } span { "{space.title}" } }
                            for item in conversations.iter().filter(|c| c.space_id.as_ref() == Some(&space.id)) {
                                ConversationRow { key: "{item.id}", item: (*item).clone() }
                            }
                        }
                    }
                }
                if conversations.iter().any(|c| !view.spaces.iter().any(|space| Some(&space.id) == c.space_id.as_ref())) {
                    details { class: "dx-conversation__history-group", open: true,
                        summary { ChevronDown { size: 12 } Folder { size: 16 } span { "对话" } }
                        for item in conversations.iter().filter(|c| !view.spaces.iter().any(|space| Some(&space.id) == c.space_id.as_ref())) {
                            ConversationRow { key: "{item.id}", item: (*item).clone() }
                        }
                    }
                }
            }

            footer { class: "dx-conversation__sidebar-footer",
                Button { variant: ButtonVariant::Ghost, onclick: move |_| state.write().dialog = Some(Dialog::Settings),
                    Settings { size: 18 } span { "设置" }
                }
            }
        }
    }
}

/// 单条会话使用稳定 ID，删除入口保留共享确认对话框。
#[component]
fn ConversationRow(item: Conversation) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let view = state.read();
    let selected = view
        .thread
        .as_ref()
        .is_some_and(|t| t.conversation.id == item.id);
    let id = item.id;
    let title = item.title.clone();
    rsx! {
        div { class: "dx-conversation__history-row", "data-selected": selected,
            Button { variant: ButtonVariant::Ghost, disabled: view.busy, title: item.title.clone(),
                aria_current: if selected { "page" } else { "false" },
                onclick: move |_| state::run(state, async move { state::select(state, id).await }),
                span { "{item.title}" }
            }
            Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost,
                aria_label: format!("删除会话 {}", item.title), title: "删除会话", disabled: view.busy,
                onclick: move |_| state.write().dialog = Some(Dialog::Delete { title: title.clone(), path: format!("/conversations/{id}") }),
                Trash2 { size: 14 }
            }
        }
    }
}
