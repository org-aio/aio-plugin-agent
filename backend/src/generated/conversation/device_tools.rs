use super::{model::Scope, service_impl::Core};
use crate::runtime::Tool;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    #[serde(default)]
    worker_id: Option<String>,
    application: String,
}

pub(super) fn tools(
    core: &Core,
    scope: &Scope,
    assistant: Uuid,
    selected: Option<Uuid>,
    prompt: &str,
) -> Vec<Arc<dyn Tool>> {
    let Some(broker) = broker(core, scope, assistant, selected, prompt) else {
        return vec![];
    };
    let mut tools: Vec<Arc<dyn Tool>> = vec![Arc::new(List(broker.clone()))];
    if core.config.gateway.as_ref().is_some_and(|g| {
        g.worker_capabilities
            .iter()
            .any(|c| c == "desktop.open-app")
    }) {
        tools.push(Arc::new(Open(broker)));
    }
    tools
}

pub(super) fn broker(
    core: &Core,
    scope: &Scope,
    assistant: Uuid,
    selected: Option<Uuid>,
    prompt: &str,
) -> Option<Arc<Broker>> {
    let gateway = core.config.gateway.as_ref()?;
    if !gateway
        .worker_capabilities
        .iter()
        .any(|c| matches!(c.as_str(), "desktop.open-app" | "workspace.execute"))
    {
        return None;
    }
    Some(Arc::new(Broker {
        client: core.client.clone(),
        token: gateway.token.clone(),
        tenant: scope.tenant.clone(),
        user: scope.user.clone(),
        assistant,
        endpoint: "http://localhost/workers".into(),
        wait: Duration::from_secs(45),
        selected,
        prompt: prompt.into(),
    }))
}

pub(super) struct Broker {
    client: reqwest::Client,
    token: String,
    tenant: String,
    user: String,
    assistant: Uuid,
    endpoint: String,
    wait: Duration,
    selected: Option<Uuid>,
    prompt: String,
}

impl Broker {
    pub(super) async fn request(&self, mut body: Value) -> Result<Value> {
        // 用户范围来自当前宿主调用上下文，不允许模型覆盖。
        body["tenantId"] = json!(self.tenant);
        body["userId"] = json!(self.user);
        let response = self
            .client
            .post(&self.endpoint)
            .header("x-aio-token", &self.token)
            .timeout(Duration::from_secs(10))
            .json(&body)
            .send()
            .await?;
        ensure!(response.status().is_success(), "设备不可用或调用未授权");
        Ok(response.json().await?)
    }

    fn request_id(&self, args: &Arguments) -> Uuid {
        let mut hash = Sha256::new();
        for part in [
            self.assistant.as_bytes().as_slice(),
            args.worker_id.as_deref().unwrap_or_default().as_bytes(),
            args.application.as_bytes(),
        ] {
            hash.update((part.len() as u64).to_be_bytes());
            hash.update(part);
        }
        let digest = hash.finalize();
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest[..16]);
        Uuid::from_bytes(bytes)
    }
}

struct List(Arc<Broker>);
struct Open(Arc<Broker>);

#[async_trait::async_trait]
impl Tool for List {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"device_list","description":"列出当前账号已授权的设备、能力及在线状态。执行前确认目标设备。","parameters":{"type":"object","properties":{},"additionalProperties":false}}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        ensure!(
            arguments.as_object().is_some_and(|v| v.is_empty()),
            "设备列表不接受参数"
        );
        let devices = self
            .0
            .request(json!({"operation":"list","capability":"*"}))
            .await?;
        ensure!(devices.is_array(), "设备列表格式无效");
        Ok(devices)
    }
}

#[async_trait::async_trait]
impl Tool for Open {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"device_open_application","description":"在用户已授权的在线设备上打开已安装应用。先调用 device_list；多台设备且用户未指定时，系统自动暂停并请求用户选择。支持打开应用，不支持编辑应用内容或任意文件操作。仅 complete 且有运行进程证明时可报告打开成功。","parameters":{"type":"object","properties":{"worker_id":{"type":"string"},"application":{"type":"string","maxLength":128}},"required":["application"],"additionalProperties":false}}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        let mut args: Arguments = serde_json::from_value(arguments)?;
        let devices = self
            .0
            .request(json!({"operation":"list","capability":"desktop.open-app"}))
            .await?;
        let device = super::device_routing::select(&devices, self.0.selected, &self.0.prompt)?;
        let id = device["id"].as_str().context("设备 ID 缺失")?;
        Uuid::parse_str(id).context("设备 ID 无效")?;
        // 路由以用户确定的目标为准，忽略模型自行猜测的设备 ID。
        args.worker_id = Some(id.into());
        ensure!(
            !args.application.trim().is_empty() && args.application.len() <= 128,
            "应用名称无效"
        );
        let id = self.0.request_id(&args);
        let mut task = self.0.request(json!({"operation":"openApp","workerId":args.worker_id,"application":args.application,"requestId":id})).await?;
        let deadline = tokio::time::Instant::now() + self.0.wait;
        loop {
            ensure!(task["id"] == id.to_string(), "设备任务 ID 不匹配");
            match task["state"].as_str().context("设备任务状态缺失")? {
                "complete" => {
                    let result = &task["result"];
                    ensure!(
                        result["running"] == true
                            && result["pid"].as_u64().is_some_and(|pid| pid > 0),
                        "设备未返回应用进程证明"
                    );
                    return Ok(
                        json!({"taskId":id,"state":"complete","result":result,"device":device}),
                    );
                }
                "queued" | "running" => {
                    if tokio::time::Instant::now() >= deadline {
                        return Ok(
                            json!({"taskId":id,"state":task["state"],"message":"设备尚未确认执行结果，不能报告已打开应用。"}),
                        );
                    }
                }
                "failed" | "cancelled" | "interrupted" => {
                    return Ok(json!({"taskId":id,"state":task["state"],"error":task["error"]}));
                }
                _ => anyhow::bail!("未知设备任务状态"),
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                continue;
            }
            match tokio::time::timeout(
                remaining,
                self.0.request(json!({"operation":"task","taskId":id})),
            )
            .await
            {
                Ok(result) => task = result?,
                Err(_) => {
                    return Ok(
                        json!({"taskId":id,"state":"pending","message":"等待设备回报超时，执行结果待确认。"}),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
