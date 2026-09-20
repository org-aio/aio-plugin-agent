use crate::{
    state::{self, AgentState},
    transport,
};
use az_agent_model::{Conversation, Thread, UserInputRequest};
use az_ui_components::{
    button::{Button, ButtonSize, ButtonVariant},
    dialog::{Dialog, DialogTitle},
    input::Input,
    select::{Select, SelectItem, SelectPlacement},
};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

#[component]
pub fn InputCard(request: UserInputRequest) -> Element {
    let state = use_context::<Signal<AgentState>>();
    let mut open = use_signal(|| true);
    rsx! {
        section { aria_label:"等待回答",
            strong { "需要你补充信息，任务已暂停" }
            Button { size:ButtonSize::Sm, variant:ButtonVariant::Outline, disabled:state.read().busy, onclick:move |_|open.set(true), "回答问题" }
            if open() { Questions { key:"{request.id}", request, on_close:move |_|open.set(false) } }
        }
    }
}

#[component]
fn Questions(request: UserInputRequest, on_close: Callback<()>) -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut answers = use_signal(BTreeMap::<String, String>::new);
    let complete = request.questions.iter().all(|question| {
        answers
            .read()
            .get(&question.id)
            .is_some_and(|v| !v.trim().is_empty())
    });
    let id = request.id;
    rsx! {
        Dialog { open:true, on_open_change:move |value:bool|if !value {on_close.call(())},
            DialogTitle { "继续任务前，请补充" }
            p { "回答后从暂停处继续。关闭窗口会保留问题。" }
            for question in request.questions {
                fieldset { key:"{question.id}",
                    legend { "{question.title}" }
                    if !question.options.is_empty() {
                        Select {
                            aria_label:question.title.clone(),
                            value:answers.read().get(&question.id).cloned().unwrap_or_default(),
                            options:std::iter::once(SelectItem::new("","请选择")).chain(question.options.iter().map(|o|SelectItem::new(&o.value,&o.label))).collect::<Vec<_>>(),
                            on_value_change:{let key=question.id.clone();move |value:String|{answers.write().insert(key.clone(),value);}},
                        }
                    }
                    if question.allow_text {
                        Input { aria_label:format!("{}：填写答案",question.title), placeholder:"也可以直接填写", value:answers.read().get(&question.id).cloned().unwrap_or_default(),
                            oninput:{let key=question.id.clone();move |event:FormEvent|{answers.write().insert(key.clone(),event.value());}},
                        }
                    }
                }
            }
            if let Some(error)=state.read().error.clone() { p { role:"alert", "{error}" } }
            Button { disabled:state.read().busy || !complete,
                onclick:move |_| {
                    let conversation=state.peek().thread.as_ref().map(|t|t.conversation.id);
                    let values=answers.peek().clone();
                    if let Some(conversation)=conversation {
                        state::run(state,async move {
                            let thread:Thread=transport::request("POST",&format!("/conversations/{conversation}/input"),json!({"requestId":id,"answers":values})).await?;
                            state.write().thread=Some(thread);
                            on_close.call(());
                            Ok(())
                        });
                    }
                }, "提交并继续"
            }
            Button { variant:ButtonVariant::Outline,disabled:state.read().busy,onclick:move |_|on_close.call(()),"稍后回答" }
            Button { variant:ButtonVariant::Ghost,disabled:state.read().busy,
                onclick:move |_| {
                    if let Some(conversation)=state.peek().thread.as_ref().map(|t|t.conversation.id) {
                        state::run(state,async move {
                            let thread:Thread=transport::request("POST",&format!("/conversations/{conversation}/cancel"),Value::Null).await?;
                            state.write().thread=Some(thread); on_close.call(()); Ok(())
                        });
                    }
                }, "取消本次任务"
            }
        }
    }
}

#[component]
pub fn ConversationDevice() -> Element {
    let mut state = use_context::<Signal<AgentState>>();
    let mut devices = use_resource(move || async move {
        transport::request::<Vec<Value>>("GET", "/devices", Value::Null).await
    });
    let thread = state.read().thread.clone();
    let selected = thread
        .as_ref()
        .and_then(|t| t.conversation.worker_id)
        .map(|v| v.to_string())
        .unwrap_or_default();
    let mut options = vec![SelectItem::new("", "自动选择 · 多台时询问")];
    if let Some(Ok(list)) = devices.read().as_ref() {
        options.extend(list.iter().filter_map(|device| {
            Some(SelectItem::new(
                device["id"].as_str()?,
                format!(
                    "{} · {} · {}",
                    device["label"].as_str().unwrap_or("设备"),
                    device["platform"].as_str().unwrap_or("未知"),
                    if device["status"] == "online" {
                        "在线"
                    } else {
                        "离线"
                    }
                ),
            ))
        }));
    }
    if !selected.is_empty() && !options.iter().any(|o| o.value == selected) {
        options.push(SelectItem::new(&selected, "选定设备不可用"));
    }
    rsx! {
        div { class:"dx-conversation__model-selector",
            Select { aria_label:"执行设备", placement: SelectPlacement::Top,value:selected,options,
                disabled:state.read().busy || state.read().processing() || thread.as_ref().is_some_and(|t|t.pending_input.is_some()),
                on_value_change:move |value:String| {
                    // 先释放只读借用，再由 run 更新忙碌状态。
                    let conversation_id = state.peek().thread.as_ref().map(|t| t.conversation.id);
                    if let Some(id) = conversation_id {
                        state::run(state,async move {
                            let worker=if value.is_empty() {None} else {Some(Uuid::parse_str(&value).map_err(|_|"设备 ID 无效")?)};
                            let conversation:Conversation=transport::request("PUT",&format!("/conversations/{id}/device"),json!({"workerId":worker})).await?;
                            if let Some(thread)=state.write().thread.as_mut() {thread.conversation=conversation;} Ok(())
                        });
                    }
                },
            }
            Button {size:ButtonSize::Sm,variant:ButtonVariant::Ghost,aria_label:"刷新设备",onclick:move |_|devices.restart(),"↻"}
            if let Some(Err(error))=devices.read().as_ref() { small {role:"alert","{error}"} }
        }
    }
}
