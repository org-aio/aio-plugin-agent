use super::{list, text, widgets::button};
use serde_json::Value;
use topcoat::{
    Result,
    view::{component, view},
};

fn status(value: &str) -> &str {
    match value {
        "queued" => "排队中",
        "running" | "inProgress" => "执行中",
        "complete" | "completed" => "已回报",
        "failed" => "失败",
        "cancelled" => "已停止",
        "interrupted" => "中断待核对",
        "cancelling" => "正在停止",
        _ => "结果待确认",
    }
}

// 设备截图只接受常见位图；保留文字回执，避免把 base64 塞进结果文本框。
fn output(result: &Value) -> (Vec<String>, String) {
    let mut result = result.clone();
    let mut images = Vec::new();
    if let Some(content) = result.get_mut("content").and_then(Value::as_array_mut) {
        for item in content {
            if text(item, "type") != "image" {
                continue;
            }
            let mime = text(item, "mimeType");
            let bytes = text(item, "data");
            if ["image/png", "image/jpeg", "image/webp", "image/gif"].contains(&mime)
                && !bytes.is_empty()
                && bytes
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+/=\r\n".contains(&b))
            {
                images.push(format!("data:{mime};base64,{bytes}"));
                *item = serde_json::json!({"type":"text","text":"截图显示于上方。"});
            }
        }
    }
    (
        images,
        serde_json::to_string_pretty(&result).unwrap_or_default(),
    )
}

#[component]
pub(super) async fn tasks(data: &Value) -> Result {
    view! {
        <p>"查看设备操作和工作区任务。已回报表示收到结果，请展开核对截图、状态和执行结果。"</p>
        for task in list(data,"tasks") {
            <details data-task=(text(task,"id"))><summary>(text(task,"label"))" · "(text(task,"device"))" · "(status(text(task,"state")))</summary>
                <p>"任务："(text(task,"id"))</p>
                if !text(task,"error").is_empty() {<p role="status">(text(task,"error"))</p>}
                if !task["result"].is_null() {
                    let (images,result)=output(&task["result"]);
                    for source in images {<img class="max-w-full h-auto" src=(source) alt=(format!("{}的设备截图",text(task,"label")))>}
                    <textarea class="dx-textarea" rows="10" readonly=(true) aria-label=(format!("{}的执行结果",text(task,"label")))>(result)</textarea>
                }
                if ["queued","running","unconfirmed","cancelling"].contains(&text(task,"state")) {button(action:"cancel-task",label:"停止任务",value:text(task,"id"))}
            </details>
        }
        if list(data,"tasks").is_empty() {<p>"还没有设备任务。在对话中说明项目和目标，让智能体派发。"</p>}
    }
}
