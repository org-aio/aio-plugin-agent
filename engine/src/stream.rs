use crate::model::{Completion, Delta, Function, ToolCall};
use anyhow::{Context, Result, bail, ensure};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc::Sender;

/// 只接受 Responses SSE；完成事件携带的输出项是工具执行与下一轮上下文的依据。
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
                    let completed = apply(data.trim_end(), &mut result, output).await?;
                    data.clear();
                    if completed {
                        return Ok(result);
                    }
                }
            } else if let Some(value) = line.strip_prefix("data:") {
                data.push_str(value.strip_prefix(' ').unwrap_or(value));
                data.push('\n');
            }
        }
    }
    bail!("模型流提前中断：缺少 response.completed")
}

async fn apply(data: &str, result: &mut Completion, output: &Sender<Delta>) -> Result<bool> {
    ensure!(data != "[DONE]", "模型流提前中断：缺少 response.completed");
    let event: Value = serde_json::from_str(data).context("模型流格式无效")?;
    match event["type"].as_str().context("模型事件类型缺失")? {
        "response.output_text.delta" | "response.refusal.delta" => {
            let text = event["delta"].as_str().context("模型文本增量无效")?;
            result.text.push_str(text);
            output.send(Delta::Text(text.into())).await?;
        }
        "response.completed" => {
            finish(&event["response"], result, output).await?;
            return Ok(true);
        }
        "response.failed"
        | "response.incomplete"
        | "response.cancelled"
        | "response.canceled"
        | "error"
        | "response.error" => bail!("模型生成失败或未完成"),
        // 中间工具参数可能尚未形成 JSON，必须等待 completed 后才执行工具。
        _ => {}
    }
    Ok(false)
}

async fn finish(response: &Value, result: &mut Completion, output: &Sender<Delta>) -> Result<()> {
    ensure!(
        response["status"] == "completed" && response["error"].is_null(),
        "模型未完成回复"
    );
    let items = response["output"].as_array().context("模型输出项缺失")?;
    let mut text = String::new();
    let mut ids = std::collections::HashSet::new();
    for (index, item) in items.iter().enumerate() {
        match item["type"].as_str() {
            Some("message") => {
                ensure!(item["role"] == "assistant", "模型输出角色无效");
                for part in item["content"].as_array().context("模型消息内容无效")? {
                    let value = match part["type"].as_str() {
                        Some("output_text") => &part["text"],
                        Some("refusal") => &part["refusal"],
                        _ => bail!("模型返回了不支持的消息内容"),
                    };
                    text.push_str(value.as_str().context("模型文本内容无效")?);
                }
            }
            Some("function_call") => {
                let id = item["call_id"].as_str().context("工具调用 ID 缺失")?;
                let name = item["name"].as_str().context("工具名称缺失")?;
                let arguments = item["arguments"].as_str().context("工具参数缺失")?;
                ensure!(
                    !id.is_empty() && id.len() <= 256 && ids.insert(id),
                    "工具调用 ID 无效"
                );
                ensure!(
                    !name.is_empty() && name.len() <= 128 && arguments.len() <= 64000,
                    "工具参数超过配额"
                );
                ensure!(result.calls.len() < 8, "单轮工具调用超过配额");
                result.calls.insert(
                    index,
                    ToolCall {
                        id: id.into(),
                        kind: "function".into(),
                        function: Function {
                            name: name.into(),
                            arguments: arguments.into(),
                        },
                    },
                );
            }
            Some("reasoning" | "compaction") => {}
            _ => bail!("模型返回了未授权的输出项类型"),
        }
    }
    // 某些网关只在完成事件给出正文；已有增量必须与最终正文一致，禁止重复输出。
    ensure!(
        text.starts_with(&result.text),
        "模型最终文本与流式增量不一致"
    );
    let remaining = &text[result.text.len()..];
    if !remaining.is_empty() {
        output.send(Delta::Text(remaining.into())).await?;
    }
    result.text = text;
    result.output = items.clone();
    result.tokens = response["usage"]["total_tokens"]
        .as_i64()
        .unwrap_or(0)
        .max(0);
    Ok(())
}
