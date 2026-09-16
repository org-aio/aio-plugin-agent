use crate::{
    state::{self, AgentState},
    transport,
};
use az_agent_model::MemoryGraph;
use az_ui_components::button::{Button, ButtonSize, ButtonVariant};
use dioxus::prelude::*;
use serde_json::json;
#[component]
pub fn GraphPanel() -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut refresh = use_signal(|| 0u64);
    let mut list = use_signal(|| false);
    let mut zoom = use_signal(|| 1.0f64);
    let thread = state.read().thread.clone();
    let space = thread
        .as_ref()
        .and_then(|t| t.conversation.space_id.clone());
    let focused = state.read().focused;
    let message = thread.as_ref().and_then(|t| {
        t.messages
            .iter()
            .find(|m| Some(m.id) == focused)
            .or_else(|| t.messages.iter().rev().find(|m| m.role == "assistant"))
    });
    let ids = message
        .map(|m| m.activated_node_ids.clone())
        .unwrap_or_default();
    let matched = message
        .map(|m| m.matched_node_ids.clone())
        .unwrap_or_default();
    let graph = use_resource(use_reactive!(|space, ids, refresh| async move {
        let _ = refresh;
        let Some(space) = space else { return Ok(None) };
        let graph: MemoryGraph = transport::memory(
            "POST",
            &format!("/activation?spaceId={space}"),
            json!({ "nodeIds" : ids.into_iter()
            .take(24).collect::< Vec < _ >> () }),
        )
        .await?;
        Ok::<_, String>(Some(graph))
    }));
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(15000).await;
            refresh += 1;
        }
    });
    rsx! {
        aside { class: "dx-conversation__context", aria_label: "知识图谱",
            header {
                strong { "知识图谱" }
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Ghost,
                    onclick: move |_| list.set(!list()),
                    if list() {
                        "图谱"
                    } else {
                        "列表"
                    }
                }
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Ghost,
                    onclick: move |_| refresh += 1,
                    "刷新"
                }
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Ghost,
                    aria_label: "收起图谱",
                    onclick: move |_| state.write().show_graph = false,
                    "×"
                }
            }
            match graph.read().as_ref() {
                Some(Ok(Some(graph))) => rsx! {
                    if graph.nodes.is_empty() {
                        p { class: "dx-conversation__empty", "暂无记忆" }
                    } else if list() {
                        div { class: "dx-conversation__graph-list",
                            for node in &graph.nodes {
                                Button {
                                    key: "{node.id}",
                                    variant: ButtonVariant::Ghost,
                                    onclick: {
                                        let id = node.id.clone();
                                        move |_| {
                                            let id = id.clone();
                                            state::run(state, async move { state::open_entry(state, id).await });
                                        }
                                    },
                                    "{node.title}"
                                }
                            }
                        }
                    } else {
                        svg {
                            class: "dx-conversation__graph",
                            view_box: format!(
                                "{} {} {} {}",
                                300.0 - 300.0 / zoom(),
                                300.0 - 300.0 / zoom(),
                                600.0 / zoom(),
                                600.0 / zoom(),
                            ),
                            role: "img",
                            "aria-label": "记忆关系图",
                            for edge in &graph.edges {
                                if let (Some(a), Some(b)) = (
                                    graph.nodes.iter().position(|n| n.id == edge.source),
                                    graph.nodes.iter().position(|n| n.id == edge.target),
                                )
                                {
                                    line {
                                        key: "{edge.id}",
                                        x1: position(a, graph.nodes.len()).0,
                                        y1: position(a, graph.nodes
                                                .len()).1,
                                        x2: position(b, graph.nodes.len()).0,
                                        y2: position(b, graph.nodes
                                                .len()).1,
                                    }
                                }
                            }
                            for (i, node) in graph.nodes.iter().enumerate() {
                                g {
                                    key: "{node.id}",
                                    class: "dx-conversation__graph-node",
                                    "data-active": ids
                                            .contains(& node.id),
                                    "data-matched": matched.contains(&node.id),
                                    role: "button",
                                    tabindex: "0",
                                    "aria-label": node.title.clone(),
                                    onclick: {
                                        let id = node.id.clone();
                                        move |_| {
                                            let id = id.clone();
                                            state::run(state, async move { state::open_entry(state, id).await });
                                        }
                                    },
                                    onkeydown: {
                                        let id = node.id.clone();
                                        move |e: KeyboardEvent| {
                                            if e.key() == Key::Enter {
                                                let id = id.clone();
                                                state::run(state, async move { state::open_entry(state, id).await });
                                            }
                                        }
                                    },
                                    circle {
                                        cx: position(i, graph.nodes.len()).0,
                                        cy: position(i, graph
                                                .nodes.len()).1,
                                        r: "13",
                                    }
                                    text {
                                        x: position(i, graph.nodes.len()).0,
                                        y: position(i, graph.nodes.len()).1 + 28.0,
                                        "{node.title.chars().take(16).collect::<String>()}"
                                    }
                                }
                            }
                        }
                    }
                    footer {
                        small {
                            "命中 {matched.len()} · 关联 {ids.len()} · 共 {graph.total}"
                        }
                        if focused.is_some() {
                            Button {
                                size: ButtonSize::Sm,
                                variant: ButtonVariant::Ghost,
                                onclick: move |_| state.write().focused = None,
                                "最新一轮"
                            }
                        }
                        Button {
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Ghost,
                            aria_label: "缩小图谱",
                            onclick: move | _ | zoom
                                    .set((zoom() - 0.2).max(0.6)),
                            "−"
                        }
                        Button {
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Ghost,
                            aria_label: "放大图谱",
                            onclick: move | _
                                    | zoom.set((zoom() + 0.2).min(2.4)),
                            "+"
                        }
                    }
                },
                Some(Ok(None)) => rsx! {
                    p { class: "dx-conversation__empty", "当前对话未关联记忆空间" }
                },
                Some(Err(error)) => rsx! {
                    p { role: "alert", "{error}" }
                },
                None => rsx! {
                    p { role: "status", "加载图谱…" }
                },
            }
        }
    }
}
fn position(index: usize, total: usize) -> (f64, f64) {
    if total == 1 {
        return (300.0, 300.0);
    }
    let angle = (index as f64 / total as f64) * std::f64::consts::TAU;
    let radius = if total > 12 && index.is_multiple_of(2) {
        115.0
    } else {
        210.0
    };
    (300.0 + radius * angle.cos(), 300.0 + radius * angle.sin())
}
