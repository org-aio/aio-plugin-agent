use crate::{Delta, InputRequired, RunState, Tool, stream};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::mpsc::Sender;

/// 请求模板由宿主注入地址、凭据与出站策略，执行器不读取进程环境或保存会话。
pub async fn run(
    request: reqwest::RequestBuilder,
    model: &str,
    messages: Vec<Value>,
    tools: Vec<Arc<dyn Tool>>,
    output: Sender<Delta>,
) -> Result<()> {
    resume(request, model, RunState::new(messages), tools, output).await
}

/// 从保存的工具边界继续，宿主在调用前重新校验身份、模型和设备权限。
pub async fn resume(
    request: reqwest::RequestBuilder,
    model: &str,
    mut state: RunState,
    tools: Vec<Arc<dyn Tool>>,
    output: Sender<Delta>,
) -> Result<()> {
    let definitions: Vec<_> = tools.iter().map(|tool| tool.definition()).collect();
    let mut names = HashSet::new();
    for definition in &definitions {
        let name = definition["function"]["name"]
            .as_str()
            .context("工具名称缺失")?;
        ensure!(!name.is_empty() && names.insert(name), "工具名称重复或为空");
    }
    loop {
        if state.pending.is_empty() {
            ensure!(state.rounds_left > 0, "Agent 已达到 8 轮执行上限");
            state.rounds_left -= 1;
            ensure!(
                serde_json::to_vec(&state.messages)?.len() <= 512000,
                "Agent 上下文超过配额"
            );
            let mut body = json!({"model":model,"messages":state.messages,"stream":true,"stream_options":{"include_usage":true}});
            if !definitions.is_empty() {
                body["tools"] = json!(definitions);
            }
            let response = request
                .try_clone()
                .context("模型请求不可重用")?
                .json(&body)
                .send()
                .await
                .context("模型服务连接失败")?;
            let completion = stream::completion(response, &output).await?;
            state.tokens = state.tokens.saturating_add(completion.tokens);
            output.send(Delta::Tokens(state.tokens)).await?;
            if completion.calls.is_empty() {
                ensure!(!completion.text.is_empty(), "模型没有返回内容");
                return Ok(());
            }
            let mut ids = HashSet::new();
            for call in completion.calls.values() {
                ensure!(
                    !call.id.is_empty() && ids.insert(&call.id),
                    "工具调用 ID 无效"
                );
            }
            state.messages.push(json!({"role":"assistant","content":completion.text,"tool_calls":completion.calls.values().collect::<Vec<_>>()}));
            state.pending = completion.calls.into_values().collect();
        }
        while let Some(call) = state.pending.first().cloned() {
            let index = definitions
                .iter()
                .position(|definition| definition["function"]["name"] == call.function.name)
                .context("模型请求了未授权工具")?;
            let parameters = &definitions[index]["function"]["parameters"];
            let empty_object = parameters["type"] == "object"
                && parameters["additionalProperties"] == false
                && parameters["properties"]
                    .as_object()
                    .is_some_and(|v| v.is_empty())
                && parameters
                    .get("required")
                    .is_none_or(|v| v.as_array().is_some_and(|v| v.is_empty()));
            let arguments = if call.function.arguments.trim().is_empty() && empty_object {
                Ok(json!({}))
            } else {
                serde_json::from_str::<Value>(&call.function.arguments)
            };
            // 参数格式错误不能执行工具；返回固定信息，让模型在轮数限制内修正。
            let value = match arguments {
                Ok(arguments) => match tools[index].invoke(arguments).await {
                    Ok(value) => value,
                    Err(error) => {
                        if let Some(input) = error.downcast_ref::<InputRequired>() {
                            output
                                .send(Delta::Waiting {
                                    state,
                                    input: input.clone(),
                                })
                                .await?;
                            return Ok(());
                        }
                        json!({"error":"工具执行失败，请检查参数、插件设置或稍后重试"})
                    }
                },
                Err(_) => {
                    json!({"error":"工具参数格式无效；请按工具 schema 发送 JSON 对象。无参数工具也请使用 {}。本次没有执行。"})
                }
            };
            let content = serde_json::to_string(&value)?;
            ensure!(content.len() <= 64000, "工具结果超过配额");
            state
                .messages
                .push(json!({"role":"tool","tool_call_id":call.id,"content":content}));
            state.pending.remove(0);
        }
    }
}
