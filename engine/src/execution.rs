use crate::{Delta, Tool, stream};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::mpsc::Sender;

/// 请求模板由宿主注入地址、凭据与出站策略，执行器不读取进程环境或保存会话。
pub async fn run(
    request: reqwest::RequestBuilder,
    model: &str,
    mut messages: Vec<Value>,
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
    let mut tokens = 0i64;
    for _ in 0..8 {
        ensure!(
            serde_json::to_vec(&messages)?.len() <= 512000,
            "Agent 上下文超过配额"
        );
        let mut body = json!({"model":model,"messages":messages,"stream":true,"stream_options":{"include_usage":true}});
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
        tokens = tokens.saturating_add(completion.tokens);
        output.send(Delta::Tokens(tokens)).await?;
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
        messages.push(json!({"role":"assistant","content":completion.text,"tool_calls":completion.calls.values().collect::<Vec<_>>()}));
        for call in completion.calls.values() {
            let index = definitions
                .iter()
                .position(|definition| definition["function"]["name"] == call.function.name)
                .context("模型请求了未授权工具")?;
            let arguments: Value =
                serde_json::from_str(&call.function.arguments).context("工具参数格式无效")?;
            // 工具错误可以由模型解释；底层异常不能把凭据、地址或内部数据带回模型。
            let value = tools[index]
                .invoke(arguments)
                .await
                .unwrap_or_else(|_| json!({"error":"工具执行失败，请检查插件设置或稍后重试"}));
            let content = serde_json::to_string(&value)?;
            ensure!(content.len() <= 64000, "工具结果超过配额");
            messages.push(json!({"role":"tool","tool_call_id":call.id,"content":content}));
        }
    }
    anyhow::bail!("Agent 已达到 8 轮执行上限")
}
