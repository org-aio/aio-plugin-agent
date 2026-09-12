mod model;
mod transport;

use anyhow::{Context, Result, ensure};
use az_agent_model::runtime::{RUNTIME_PROTOCOL_VERSION, RuntimeCommand, RuntimeEvent};
use futures_util::StreamExt;
pub use model::EngineConfig;
pub(crate) use model::{Delta, ToolHandler};
use serde_json::Value;
use std::{collections::HashSet, process::Stdio, sync::Arc};
use tokio::{
    process::Command,
    sync::{Mutex, mpsc},
    task::JoinSet,
};
use tokio_util::codec::{FramedRead, LinesCodec};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate(
    engine: &EngineConfig,
    client: &reqwest::Client,
    gateway: Option<&crate::configuration::Gateway>,
    endpoint: &str,
    model: &str,
    secret: Option<&str>,
    messages: Vec<Value>,
    tools: Option<Arc<dyn ToolHandler>>,
    output: mpsc::Sender<Delta>,
) -> Result<()> {
    let mut child = Command::new(&engine.node)
        .arg("--permission")
        .arg(format!("--allow-fs-read={}", engine.directory.display()))
        .arg(format!(
            "--allow-fs-read={}",
            engine
                .directory
                .parent()
                .context("运行时位置无效")?
                .join("node_modules")
                .display()
        ))
        .arg(engine.directory.join("main.mjs"))
        .current_dir(&engine.directory)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("PI_OFFLINE", "1")
        .env("LANG", "C.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("无法启动 Agent 运行时")?;
    let input = Arc::new(Mutex::new(child.stdin.take().context("运行时输入不可用")?));
    let mut reader = FramedRead::new(
        child.stdout.take().context("运行时输出不可用")?,
        LinesCodec::new_with_max_length(256000),
    );
    transport::send(
        &input,
        RuntimeCommand::Start {
            version: RUNTIME_PROTOCOL_VERSION,
            model,
            messages,
            tools: if tools.is_some() {
                vec!["memory_search"]
            } else {
                vec![]
            },
        },
    )
    .await?;
    let mut pending = JoinSet::new();
    let mut requests = HashSet::new();
    let mut total = 0;
    loop {
        let line = tokio::select! {
            result = pending.join_next(), if !pending.is_empty() => {
                result.context("出站任务丢失")?.context("出站任务中断")??;
                continue;
            }
            line = reader.next() => line.context("Agent 运行时提前退出")?.map_err(|_| anyhow::anyhow!("Agent 运行时协议无效"))?,
        };
        total += line.len();
        ensure!(total <= 4 * 1024 * 1024, "Agent 运行时输出超过配额");
        let event: RuntimeEvent =
            serde_json::from_str(&line).map_err(|_| anyhow::anyhow!("Agent 运行时协议无效"))?;
        match event {
            RuntimeEvent::Fetch { id, url, body } => {
                ensure!(
                    requests.insert(id) && requests.len() <= 16,
                    "Agent 请求次数超过配额"
                );
                ensure!(
                    url == "https://aio.invalid/v1/chat/completions"
                        && body["model"] == model
                        && body["stream"] == true,
                    "Agent 请求超出模型授权"
                );
                pending.spawn(transport::forward(
                    client.clone(),
                    gateway.cloned(),
                    endpoint.into(),
                    secret.map(str::to_owned),
                    body,
                    id,
                    input.clone(),
                ));
            }
            RuntimeEvent::Tool {
                id,
                name,
                arguments,
            } => {
                ensure!(
                    requests.insert(id) && requests.len() <= 16 && name == "memory_search",
                    "Agent 工具未授权"
                );
                let tools = tools.clone().context("当前任务未授权工具")?;
                let input = input.clone();
                pending.spawn(async move {
                    let value = tools
                        .invoke(&name, arguments)
                        .await
                        .unwrap_or_else(|_| serde_json::json!({"error":"资料不可用或无权访问"}));
                    transport::send(&input, RuntimeCommand::ToolResult { id, value }).await
                });
            }
            RuntimeEvent::Text { text } => output.send(Delta::Text(text)).await?,
            RuntimeEvent::Usage { tokens } => output.send(Delta::Tokens(tokens.max(0))).await?,
            RuntimeEvent::Error => anyhow::bail!("Agent 模型执行失败"),
            RuntimeEvent::Done => break,
        }
    }
    while let Some(result) = pending.join_next().await {
        result.context("出站任务中断")??;
    }
    ensure!(child.wait().await?.success(), "Agent 运行时退出失败");
    Ok(())
}
