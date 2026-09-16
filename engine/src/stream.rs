use crate::model::{Chunk, Completion, Delta};
use anyhow::{Context, Result, ensure};
use futures_util::StreamExt;
use tokio::sync::mpsc::Sender;

pub(crate) async fn completion(
    response: reqwest::Response,
    output: &Sender<Delta>,
) -> Result<Completion> {
    ensure!(
        response.status().is_success(),
        "模型服务返回 HTTP {}",
        response.status().as_u16()
    );
    ensure!(
        response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream")),
        "模型服务没有返回 SSE 流"
    );
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut data = String::new();
    let mut total = 0;
    let mut result = Completion::default();
    let mut done = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("读取模型数据失败")?;
        total += chunk.len();
        ensure!(total <= 2 * 1024 * 1024, "模型数据超过配额");
        buffer.extend_from_slice(&chunk);
        while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
            let bytes: Vec<_> = buffer.drain(..=end).collect();
            let line = std::str::from_utf8(&bytes)
                .context("模型流编码无效")?
                .trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if !data.is_empty() {
                    done = apply(data.trim_end(), &mut result, output).await?;
                    data.clear();
                    if done {
                        break;
                    }
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                data.push_str(value.strip_prefix(' ').unwrap_or(value));
                data.push('\n');
            }
        }
        if done {
            break;
        }
    }
    ensure!(done && result.finished, "模型流提前中断");
    Ok(result)
}

async fn apply(data: &str, result: &mut Completion, output: &Sender<Delta>) -> Result<bool> {
    if data == "[DONE]" {
        return Ok(true);
    }
    let chunk: Chunk = serde_json::from_str(data).context("模型流格式无效")?;
    ensure!(chunk.error.is_none(), "模型生成失败");
    if let Some(usage) = chunk.usage {
        result.tokens = usage.total_tokens.max(0);
    }
    if let Some(choice) = chunk.choices.into_iter().next() {
        if let Some(reason) = choice.finish_reason {
            ensure!(
                matches!(reason.as_str(), "stop" | "tool_calls"),
                "模型未完成回复：{reason}"
            );
            result.finished = true;
        }
        if let Some(text) = choice.delta.content {
            result.text.push_str(&text);
            output.send(Delta::Text(text)).await?;
        }
        for change in choice.delta.tool_calls {
            ensure!(change.index < 8, "单轮工具调用超过配额");
            let call = result.calls.entry(change.index).or_default();
            call.kind = "function".into();
            if let Some(id) = change.id {
                call.id.push_str(&id);
            }
            if let Some(function) = change.function {
                if let Some(name) = function.name {
                    call.function.name.push_str(&name);
                }
                if let Some(arguments) = function.arguments {
                    call.function.arguments.push_str(&arguments);
                }
            }
            ensure!(
                call.id.len() <= 256
                    && call.function.name.len() <= 128
                    && call.function.arguments.len() <= 64000,
                "工具参数超过配额"
            );
        }
    }
    Ok(false)
}
