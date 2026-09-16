use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

pub(crate) use az_agent_engine::{Delta, Tool};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn generate(
    client: &reqwest::Client,
    gateway: Option<&crate::configuration::Gateway>,
    endpoint: &str,
    model: &str,
    secret: Option<&str>,
    messages: Vec<Value>,
    tools: Vec<Arc<dyn Tool>>,
    output: mpsc::Sender<Delta>,
) -> Result<()> {
    continue_run(
        client,
        gateway,
        endpoint,
        model,
        secret,
        az_agent_engine::RunState::new(messages),
        tools,
        output,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn continue_run(
    client: &reqwest::Client,
    gateway: Option<&crate::configuration::Gateway>,
    endpoint: &str,
    model: &str,
    secret: Option<&str>,
    state: az_agent_engine::RunState,
    tools: Vec<Arc<dyn Tool>>,
    output: mpsc::Sender<Delta>,
) -> Result<()> {
    let mut request = if let Some(gateway) = gateway {
        client
            .post("http://localhost/egress")
            .header("x-aio-endpoint", endpoint)
            .header("x-aio-token", &gateway.token)
    } else {
        client.post(format!(
            "{}/chat/completions",
            endpoint.trim_end_matches('/')
        ))
    };
    if let Some(secret) = secret {
        request = request.bearer_auth(secret);
    }
    az_agent_engine::resume(request, model, state, tools, output).await
}
