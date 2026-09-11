use anyhow::{Context, Result, ensure};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;

pub enum Delta {
    Text(String),
    Tokens(i64),
}

pub async fn generate(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
    secret: Option<&str>,
    messages: Vec<Value>,
    output: mpsc::Sender<Delta>,
) -> Result<()> {
    let mut request = client.post(format!("{}/chat/completions", endpoint.trim_end_matches('/')))
        .json(&json!({"model":model, "messages":messages, "stream":true, "stream_options":{"include_usage":true}}));
    if let Some(secret) = secret {
        request = request.bearer_auth(secret);
    }
    let response = request.send().await.context("模型服务连接失败")?;
    ensure!(
        response.status().is_success(),
        "模型服务返回 HTTP {}",
        response.status().as_u16()
    );
    ensure!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s.starts_with("text/event-stream")),
        "模型服务没有返回 SSE 流"
    );
    let mut raw_bytes = 0usize;
    let stream = response.bytes_stream().map(move |chunk| {
        let chunk = chunk.map_err(anyhow::Error::from)?;
        raw_bytes += chunk.len();
        ensure!(raw_bytes <= 2 * 1024 * 1024, "模型数据超过配额");
        Ok::<_, anyhow::Error>(chunk)
    });
    let mut events = stream.eventsource();
    let mut complete = false;
    let mut bytes = 0usize;
    let mut events_count = 0;
    while let Some(event) = events.next().await {
        let event = event.context("读取模型数据失败")?;
        events_count += 1;
        bytes += event.data.len();
        ensure!(
            bytes <= 2 * 1024 * 1024 && events_count <= 20000,
            "模型输出超过配额"
        );
        if event.data.trim() == "[DONE]" {
            complete = true;
            break;
        }
        let value: Value = serde_json::from_str(&event.data).context("模型返回无效 JSON")?;
        ensure!(value.get("error").is_none(), "模型服务返回错误事件");
        if let Some(tokens) = value["usage"]["total_tokens"].as_i64() {
            output.send(Delta::Tokens(tokens.max(0))).await?;
        }
        if let Some(choice) = value["choices"].as_array().and_then(|v| v.first()) {
            ensure!(
                choice["delta"]["tool_calls"].is_null(),
                "当前对话没有授权工具执行"
            );
            if let Some(text) = choice["delta"]["content"]
                .as_str()
                .or_else(|| choice["delta"]["refusal"].as_str())
            {
                output.send(Delta::Text(text.into())).await?;
            }
        }
    }
    ensure!(complete, "模型流提前中断");
    Ok(())
}
