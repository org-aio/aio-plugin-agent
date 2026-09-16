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
    worker_id: String,
    application: String,
}

pub(super) fn tools(core: &Core, scope: &Scope, assistant: Uuid) -> Vec<Arc<dyn Tool>> {
    let Some(gateway) = core.config.gateway.as_ref() else {
        return vec![];
    };
    if !gateway
        .worker_capabilities
        .iter()
        .any(|item| item == "desktop.open-app")
    {
        return vec![];
    }
    let broker = Arc::new(Broker {
        client: core.client.clone(),
        token: gateway.token.clone(),
        tenant: scope.tenant.clone(),
        user: scope.user.clone(),
        assistant,
        endpoint: "http://localhost/workers".into(),
        wait: Duration::from_secs(45),
    });
    vec![Arc::new(List(broker.clone())), Arc::new(Open(broker))]
}

struct Broker {
    client: reqwest::Client,
    token: String,
    tenant: String,
    user: String,
    assistant: Uuid,
    endpoint: String,
    wait: Duration,
}

impl Broker {
    async fn request(&self, mut body: Value) -> Result<Value> {
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
            args.worker_id.as_bytes(),
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
        json!({"type":"function","function":{"name":"device_list","description":"列出当前账号已授权应用控制的设备及在线状态。打开应用前先选择设备。","parameters":{"type":"object","properties":{},"additionalProperties":false}}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        ensure!(
            arguments.as_object().is_some_and(|v| v.is_empty()),
            "设备列表不接受参数"
        );
        let devices = self.0.request(json!({"operation":"list"})).await?;
        ensure!(devices.is_array(), "设备列表格式无效");
        Ok(devices)
    }
}

#[async_trait::async_trait]
impl Tool for Open {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"device_open_application","description":"在用户已授权的在线设备上打开已安装应用。先调用 device_list；多台设备且用户未指定时先询问。仅 complete 且有运行进程证明时可报告打开成功。","parameters":{"type":"object","properties":{"worker_id":{"type":"string"},"application":{"type":"string","maxLength":128}},"required":["worker_id","application"],"additionalProperties":false}}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        let args: Arguments = serde_json::from_value(arguments)?;
        Uuid::parse_str(&args.worker_id).context("设备 ID 无效")?;
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
                    return Ok(json!({"taskId":id,"state":"complete","result":result}));
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
