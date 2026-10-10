use super::text;
use serde_json::Value;
use topcoat::{
    Result,
    view::{component, view},
};

#[component]
pub(super) async fn icon(name: &str) -> Result {
    let paths: &[&str] = match name {
        "new" => &[
            "M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7",
            "m16 3 5 5-9 9-5 1 1-5Z",
        ],
        "search" => &["m21 21-4.3-4.3", "M19 11a8 8 0 1 1-16 0 8 8 0 0 1 16 0"],
        "sidebar" => &["M3 5h18v14H3Z", "M9 5v14"],
        "delete-thread" | "delete-skill" => &[
            "M3 6h18",
            "M9 6V3h6v3",
            "m5 6 1 15h12l1-15",
            "M10 10v7m4-7v7",
        ],
        "settings" | "environment" => &["M3 5h18v14H3Z", "M15 5v14"],
        "add-workspace" => &["M12 5v14M5 12h14"],
        "folder" => &["M3 7V4h6l2 3h10v13H3Z"],
        "spaces" | "graph" | "skills" => &["M12 4v16M4 12h16", "M7 3h10v18H7Z"],
        "refresh-models" | "refresh-devices" | "refresh-panel" | "reconnect" => {
            &["M20 7a9 9 0 1 0 1 7", "M20 3v5h-5"]
        }
        "copy-message" => &["M9 9h12v12H9Z", "M15 9V3H3v12h6"],
        "chevron" => &["m6 9 6 6 6-6"],
        _ => &[],
    };
    view! { <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">for path in paths {<path d=(*path)></path>}</svg> }
}

#[component]
pub(super) async fn button(
    action: &str,
    #[into] label: String,
    #[default]
    #[into]
    value: String,
    #[default] disabled: bool,
    #[default] icon_only: bool,
) -> Result {
    view! { <button type="button" class="dx-button" data-style="ghost" data-size=(if icon_only {"icon-sm"}else{"sm"}) data-action=(action) data-value=(value) aria-label=(&label) title=(&label) disabled=(disabled)>
        if icon_only||["new","search","spaces","skills","settings"].contains(&action) { icon(name:action) }
        if !icon_only {<span>(&label)</span>}
    </button> }
}

#[component]
pub(super) async fn field(
    #[into] name: String,
    #[into] label: String,
    #[default]
    #[into]
    value: String,
    #[default("text")] kind: &str,
    #[default] required: bool,
) -> Result {
    view! { <label>(&label)<input class="dx-input" name=(name) type=(kind) value=(value) aria-label=(&label) required=(required) autocomplete="off"></label> }
}

/// 复用共享 Select 的 DOM 和样式契约；浏览器脚本只管理展开、键盘和选中值。
#[component]
pub(super) async fn select(
    name: &str,
    label: &str,
    placeholder: &str,
    items: &[Value],
    selected: &str,
    #[default] disabled: bool,
) -> Result {
    let current = items
        .iter()
        .find(|item| text(item, "id") == selected)
        .map(|item| text(item, "label"))
        .unwrap_or(placeholder);
    view! {
        <div class="dx-select" data-placement="top" data-state="closed" data-disabled=(disabled)>
            <button type="button" class="dx-select-trigger" aria-label=(label) aria-haspopup="listbox" aria-expanded="false" data-action="menu-toggle" disabled=(disabled)><span>(current)</span>icon(name:"chevron")</button>
            <div class="dx-select-list" role="listbox" aria-label=(label) hidden=(true)>
                <div class="dx-select-option" role="option" tabindex="0" aria-selected=(selected.is_empty()) data-action="choose" data-field=(name) data-value=""><span>(placeholder)</span></div>
                for item in items {
                    <div class="dx-select-option" role="option" tabindex=(if item["disabled"]==true {"-1"}else{"0"}) aria-selected=(selected==text(item,"id")) aria-disabled=(item["disabled"]==true) data-disabled=(item["disabled"]==true) data-action="choose" data-field=(name) data-value=(text(item,"id"))><span>(text(item,"label"))</span>if selected==text(item,"id"){<span>"✓"</span>}</div>
                }
            </div>
        </div>
    }
}
