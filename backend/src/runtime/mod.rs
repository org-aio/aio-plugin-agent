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
    az_agent_engine::run(request, model, messages, tools, output).await
}
