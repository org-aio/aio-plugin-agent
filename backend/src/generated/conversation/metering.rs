use super::{model::Scope, service_impl::Core};
use az_plugin_contract::process::MeterRequest;
use serde::Deserialize;

/// 计费资源名，与宿主 `billing_prices` 表中的键一致。
const TOKEN_RESOURCE: &str = "agent_tokens";

/// 上报一次生成消耗的 token。
///
/// 只有经宿主 broker 运行的实例才有 gateway；本机开发实例直接跳过，
/// 计费失败不改变生成结果，但会在日志中留下记录。
pub(super) async fn record_tokens(
    core: &Core,
    scope: &Scope,
    message: uuid::Uuid,
    tokens: Option<i64>,
) {
    let Some(quantity) = tokens.filter(|value| *value > 0) else {
        return;
    };
    let Some(gateway) = core.config.gateway.as_ref() else {
        return;
    };
    let request = MeterRequest {
        tenant_id: scope.tenant.clone(),
        user_id: scope.user.clone(),
        source_id: "agent".to_owned(),
        resource: TOKEN_RESOURCE.to_owned(),
        quantity,
        idempotency_key: message.to_string(),
    };
    let result = core
        .client
        .post("http://localhost/meter")
        .header("x-aio-token", &gateway.token)
        .json(&request)
        .send()
        .await;
    match result {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => {
            eprintln!("计费用量上报失败：宿主返回 {}", response.status());
        }
        Err(error) => {
            eprintln!("计费用量上报失败：{error}");
        }
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct MeterResponse {
    status: u16,
    body: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_request_uses_camel_case_and_message_idempotency() {
        let request = MeterRequest {
            tenant_id: "tenant-1".into(),
            user_id: "user-1".into(),
            source_id: "agent".into(),
            resource: TOKEN_RESOURCE.into(),
            quantity: 1_234,
            idempotency_key: "msg-1".into(),
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["tenantId"], "tenant-1");
        assert_eq!(value["sourceId"], "agent");
        assert_eq!(value["resource"], "agent_tokens");
        assert_eq!(value["quantity"], 1_234);
        assert_eq!(value["idempotencyKey"], "msg-1");
    }
}
