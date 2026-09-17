//! 将已有持久化检查点一次性升级为 Responses 输入项，不保留旧协议出站路径。
use crate::RunState;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(crate) fn upgrade(state: &mut RunState) -> Result<()> {
    ensure!(state.protocol_version <= 1, "执行检查点版本不受支持");
    if state.protocol_version == 1 {
        ensure!(
            state
                .observation_indices
                .iter()
                .all(|index| *index < state.messages.len()),
            "桌面观察检查点无效"
        );
        return Ok(());
    }
    let mut items = Vec::new();
    let mut observations = Vec::new();
    for message in &state.messages {
        if message["role"] == "tool" {
            items.push(json!({"type":"function_call_output","call_id":message["tool_call_id"],"output":message["content"]}));
            continue;
        }
        if message["name"] == "device_observation" {
            observations.push(items.len());
        }
        let mut item = message.clone();
        let object = item.as_object_mut().context("执行检查点消息无效")?;
        object.remove("name");
        let calls = object.remove("tool_calls");
        if let Some(parts) = object.get_mut("content").and_then(Value::as_array_mut) {
            for part in parts {
                upgrade_part(part);
            }
        }
        if !item["content"].is_null() {
            items.push(item);
        }
        if let Some(calls) = calls {
            for call in calls.as_array().context("执行检查点工具调用无效")? {
                items.push(json!({"type":"function_call","call_id":call["id"],"name":call["function"]["name"],"arguments":call["function"]["arguments"]}));
            }
        }
    }
    for part in &mut state.observations {
        upgrade_part(part);
    }
    state.messages = items;
    state.observation_indices = observations;
    state.protocol_version = 1;
    Ok(())
}

fn upgrade_part(part: &mut Value) {
    match part["type"].as_str() {
        Some("text") => part["type"] = json!("input_text"),
        Some("image_url") => {
            let url = part["image_url"]["url"].clone();
            let detail = part["image_url"]
                .get("detail")
                .cloned()
                .unwrap_or(json!("auto"));
            *part = json!({"type":"input_image","image_url":url,"detail":detail});
        }
        _ => {}
    }
}
