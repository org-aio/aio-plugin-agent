use crate::state::AgentState;
use az_ui_components::button::{Button, ButtonSize, ButtonVariant};
use dioxus::prelude::*;
use dioxus_icons::lucide::X;

/// 右侧信息栏只展示当前会话的真实配置，不推测宿主路径、分支或权限。
#[component]
pub(super) fn EnvironmentPanel(on_close: EventHandler<MouseEvent>) -> Element {
    let state = use_context::<Signal<AgentState>>();
    let view = state.read();
    let conversation = view.thread.as_ref().map(|thread| &thread.conversation);
    let provider = view.settings.as_ref().and_then(|settings| {
        settings
            .providers
            .iter()
            .find(|provider| Some(provider.id) == conversation.and_then(|c| c.provider_id))
    });
    let model = conversation
        .and_then(|c| c.model.as_deref())
        .or_else(|| provider.map(|p| p.model.as_str()))
        .unwrap_or("跟随空间模型");
    let space = view
        .spaces
        .iter()
        .find(|space| Some(&space.id) == conversation.and_then(|c| c.space_id.as_ref()));
    rsx! {
        aside { class: "dx-conversation__environment", aria_label: "环境信息",
            header { strong { "环境信息" }
                Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost, aria_label: "关闭环境信息", onclick: on_close, X { size: 16 } }
            }
            dl {
                dt { "运行环境" } dd { "AIO Agent" }
                dt { "模型服务" } dd { "{provider.map(|p| p.label.as_str()).unwrap_or(\"由记忆空间决定\")}" }
                dt { "对话模型" } dd { "{model}" }
                dt { "记忆空间" } dd { "{space.map(|s| s.title.as_str()).unwrap_or(\"未关联\")}" }
                dt { "执行设备" }
                dd {
                    if conversation.is_some_and(|c| c.worker_id.is_some()) { "已指定设备" } else { "自动选择 · 多台时询问" }
                }
                dt { "对话状态" }
                dd {
                    if view.thread.as_ref().is_some_and(|t| t.pending_input.is_some()) { "等待回答" }
                    else if view.processing() { "正在处理" }
                    else { "就绪" }
                }
            }
        }
    }
}
