mod views;
use serde::Deserialize;
use serde_json::Value;
use topcoat::{context::Cx, view::view};

/// 浏览器只提交显示状态；此入口不执行任务，也不读取其他用户的数据。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub section: String,
    #[serde(default)]
    pub data: Value,
}

pub async fn render(request: RenderRequest) -> anyhow::Result<String> {
    let cx = &Cx::default();
    let data = &request.data;
    let page = match request.section.as_str() {
        "shell" => views::shell::document(data.as_str().unwrap_or("chat")).await,
        "history" => view! { cx => views::conversation::history(data: data) },
        "messages" => view! { cx => views::conversation::messages(data: data) },
        "message" => view! { cx => views::conversation::message(data: data) },
        "message-header" => view! { cx => views::conversation::message_header(data: data) },
        "message-footer" => view! { cx => views::conversation::message_footer(data: data) },
        "controls" => view! { cx => views::conversation::controls(data: data) },
        "models" => view! { cx => views::conversation::models(data:data) },
        "shortcuts" => view! { cx => views::conversation::shortcuts(data:data) },
        "settings" => view! { cx => views::settings::settings(data: data) },
        "dialog" => view! { cx => views::settings::dialog(data: data) },
        "skills" => view! { cx => views::skills::library(data: data) },
        "memory" => view! { cx => views::memory::panel(data: data) },
        _ => anyhow::bail!("不支持的界面片段"),
    }
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    Ok(page.render(&Cx::default()))
}
