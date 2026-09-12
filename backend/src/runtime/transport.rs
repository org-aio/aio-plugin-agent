use anyhow::{Context, Result, ensure};
use az_agent_model::runtime::RuntimeCommand;
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use tokio::{io::AsyncWriteExt, process::ChildStdin, sync::Mutex};

pub(super) type Input = Arc<Mutex<ChildStdin>>;

pub(super) async fn send(input: &Input, message: RuntimeCommand<'_>) -> Result<()> {
    let mut line = serde_json::to_vec(&message)?;
    line.push(b'\n');
    let mut input = input.lock().await;
    input.write_all(&line).await.context("Agent 运行时已断开")?;
    input.flush().await?;
    Ok(())
}

pub(super) async fn forward(
    client: reqwest::Client,
    endpoint: String,
    secret: Option<String>,
    body: Value,
    id: u32,
    input: Input,
) -> Result<()> {
    let mut request = client
        .post(format!(
            "{}/chat/completions",
            endpoint.trim_end_matches('/')
        ))
        .json(&body);
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
            .is_some_and(|v| v.starts_with("text/event-stream")),
        "模型服务没有返回 SSE 流"
    );
    send(
        &input,
        RuntimeCommand::Response {
            id,
            status: response.status().as_u16(),
        },
    )
    .await?;
    let mut bytes = 0;
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.context("读取模型数据失败")?;
        bytes += chunk.len();
        ensure!(bytes <= 2 * 1024 * 1024, "模型数据超过配额");
        for part in chunk.chunks(16384) {
            send(
                &input,
                RuntimeCommand::Chunk {
                    id,
                    data: STANDARD.encode(part),
                },
            )
            .await?;
        }
    }
    send(&input, RuntimeCommand::End { id }).await
}
