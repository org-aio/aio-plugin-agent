use crate::{state::AgentState, transport};
use az_agent_model::SwarmTask;
use az_ui_components::{
    button::{Button, ButtonSize, ButtonVariant},
    dialog::{Dialog, DialogTitle},
    textarea::Textarea,
};
use dioxus::prelude::*;
use serde_json::Value;
use uuid::Uuid;

#[component]
pub fn SwarmTasks(conversation: Uuid) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        Button { size:ButtonSize::Sm, variant:ButtonVariant::Outline, onclick:move |_|open.set(true), "蜂群任务" }
        if open() { TaskDialog { key:"{conversation}", conversation, on_close:move |_|open.set(false) } }
    }
}
fn status(value: &str) -> &str {
    match value {
        "queued" => "排队中",
        "running" => "执行中",
        "complete" => "已回报",
        "failed" => "失败",
        "cancelled" => "已停止",
        "interrupted" => "中断待核对",
        "cancelling" => "正在停止",
        _ => "结果待确认",
    }
}
#[component]
fn TaskDialog(conversation: Uuid, on_close: Callback<()>) -> Element {
    let state = use_context::<Signal<AgentState>>();
    let mut tasks = use_signal(Vec::<SwarmTask>::new);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let mut stopping = use_signal(|| None::<Uuid>);
    use_future(move || async move {
        loop {
            match transport::request::<Vec<SwarmTask>>(
                "GET",
                &format!("/conversations/{conversation}/tasks"),
                Value::Null,
            )
            .await
            {
                Ok(value) => {
                    tasks.set(value);
                    error.set(None);
                }
                Err(value) => error.set(Some(value)),
            }
            loading.set(false);
            gloo_timers::future::TimeoutFuture::new(2000).await;
        }
    });
    rsx! {
        Dialog { open:true, on_open_change:move |value:bool|if !value { on_close.call(()) },
            DialogTitle { "蜂群任务" }
            p { "按设备和工作区执行独立任务。已回报表示收到结果，请展开检查各项状态、退出码和日志。" }
            if let Some(error)=error() { p { role:"alert", "{error}" } }
            if loading() { p { "正在读取设备回执…" } }
            if !loading() && tasks.read().is_empty() { p { "还没有设备任务。在对话中说明项目和目标，让智能体派发。" } }
            for task in tasks() {
                details { key:"{task.id}",
                    summary { "{task.label} · {task.device} · {status(&task.state)}" }
                    p { "任务：{task.id}" }
                    if let Some(message)=task.error { p { role:"status", "{message}" } }
                    if let Some(result)=task.result {
                        Textarea { aria_label:format!("{}的执行结果",task.label), value:serde_json::to_string_pretty(&result).unwrap_or_default(), readonly:true, rows:10 }
                    }
                    if matches!(task.state.as_str(), "queued" | "running" | "unconfirmed" | "cancelling") {
                        Button { variant:ButtonVariant::Outline, disabled:stopping().is_some(),
                            onclick:move |_| {
                                stopping.set(Some(task.id));
                                spawn(async move {
                                    let result=transport::request::<Value>("POST",&format!("/conversations/{conversation}/tasks/{}/cancel",task.id),Value::Null).await;
                                    if let Err(value)=result {error.set(Some(value));}
                                    stopping.set(None);
                                });
                            }, "停止任务"
                        }
                    }
                }
            }
            if state.read().running() { p { "主智能体仍在执行，独立子任务会继续更新。" } }
            Button { variant:ButtonVariant::Outline, onclick:move |_|on_close.call(()), "关闭" }
        }
    }
}
