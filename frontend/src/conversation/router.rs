use dioxus::prelude::*;

/// 复现路由面板的位置与密度；没有执行协议时不允许启用隐私或规划能力。
#[component]
pub(super) fn RouterPanel() -> Element {
    rsx! {
        details { class: "dx-conversation__router", open: true,
            summary { b { "Auto Router" } span { "当前执行器尚未接入自动路由" } }
            div { class: "dx-conversation__router-panel",
                fieldset { class: "dx-conversation__router-settings", disabled: true,
                    legend { class: "dx-conversation__visually-hidden", "自动路由设置（当前不可用）" }
                    label { span { "隐私" } input { r#type: "checkbox", role: "switch", aria_label: "隐私（未接入）" } }
                    label { span { "自动规划" } input { r#type: "checkbox", role: "switch", aria_label: "自动规划（未接入）" } }
                    label { span { "精确命令旁路" } input { r#type: "checkbox", role: "switch", aria_label: "精确命令旁路（未接入）" } }
                    label { span { "执行角色" } select { aria_label: "执行角色（未接入）", option { "自动选择角色" } } }
                    label { span { "夯 · 规划模型" } select { aria_label: "规划模型（未接入）", option { "动态选择" } } }
                    label { span { "垃 · 执行模型" } select { aria_label: "执行模型（未接入）", option { "跟随对话模型" } } }
                }
                p { class: "dx-conversation__router-note", "当前消息使用所选对话模型；隐私和规划开关尚不可用。" }
            }
        }
    }
}
