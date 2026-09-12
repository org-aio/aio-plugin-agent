use super::{model::Scope, service_impl::Core};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

pub async fn safe_thread(
    core: &Core,
    scope: &Scope,
    id: uuid::Uuid,
    strict: bool,
) -> super::model::ServiceResult<super::model::Thread> {
    let mut thread = super::store::thread(&core.pool, scope, id).await?;
    let Some(space) = &thread.conversation.space_id else {
        return Ok(thread);
    };
    let ids: HashSet<_> = thread
        .messages
        .iter()
        .flat_map(|message| {
            message
                .source_id
                .iter()
                .chain(message.citations.iter().map(|citation| &citation.id))
                .chain(message.activated_node_ids.iter())
                .chain(message.matched_node_ids.iter())
        })
        .cloned()
        .collect();
    if ids.is_empty() {
        return Ok(thread);
    }
    let ids: Vec<_> = ids.into_iter().collect();
    let mut visible = HashSet::<String>::new();
    for batch in ids.chunks(400) {
        let result = invoke(
            core,
            scope,
            "POST",
            &format!("/visibility?spaceId={space}"),
            json!({"nodeIds":batch}),
            false,
        )
        .await;
        match result {
            Ok(value) => visible
                .extend(serde_json::from_value::<Vec<String>>(value).context("引用校验响应无效")?),
            Err(error) if strict => return Err(error.into()),
            Err(_) => {}
        }
    }
    for message in &mut thread.messages {
        if message
            .source_id
            .as_ref()
            .is_some_and(|source| !visible.contains(source))
            || message
                .citations
                .iter()
                .any(|citation| !visible.contains(&citation.id))
        {
            message.content = "[来源已删除或当前不可访问]".into();
            message.citations.clear();
            message.activated_node_ids.clear();
            message.matched_node_ids.clear();
            message.memory_status = Some("unavailable".into());
            message.error = None;
        }
        message.activated_node_ids.retain(|id| visible.contains(id));
        message.matched_node_ids.retain(|id| visible.contains(id));
    }
    Ok(thread)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedSource {
    pub id: String,
    pub space_id: String,
    pub text: String,
    pub status: String,
}

pub async fn invoke(
    core: &Core,
    scope: &Scope,
    method: &str,
    path: &str,
    body: Value,
    interactive: bool,
) -> Result<Value> {
    let connection = core.config.memory.as_ref().context("记忆服务尚未绑定")?;
    let response = core
        .client
        .post(connection.endpoint.clone())
        .bearer_auth(&connection.token)
        .header("x-aio-token", &connection.token)
        .json(&az_plugin_contract::process::ServiceRequest {
            target: crate::hosting::MEMORY_SOURCE.into(),
            method: method.into(),
            path: path.into(),
            body,
            tenant_id: scope.tenant.clone(),
            user_id: scope.user.clone(),
            context_id: scope.context_id.clone(),
            interactive,
        })
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .context("记忆服务暂不可用")?;
    ensure!(response.status().is_success(), "记忆服务拒绝请求");
    ensure!(
        response.content_length().unwrap_or(0) <= 2 * 1024 * 1024,
        "记忆响应超过配额"
    );
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 2 * 1024 * 1024,
            "记忆响应超过配额"
        );
        bytes.extend_from_slice(&chunk);
    }
    let envelope: Value = serde_json::from_slice(&bytes).context("记忆服务响应无效")?;
    let status = envelope["status"].as_u64().context("记忆服务状态无效")?;
    ensure!((200..300).contains(&status), "记忆操作暂不可用或无权访问");
    Ok(envelope["body"].clone())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRoute {
    pub route: String,
    pub reply: Option<String>,
    pub context: String,
    pub citations: Vec<super::model::MemoryCitation>,
    pub matched_node_ids: Vec<String>,
    pub activated_node_ids: Vec<String>,
}

pub async fn route(core: &Core, scope: &Scope, space: &str, source: &str) -> Result<ChatRoute> {
    let result = invoke(
        core,
        scope,
        "POST",
        &format!("/route?spaceId={space}"),
        json!({"sourceId":source}),
        false,
    )
    .await?;
    serde_json::from_value(result).context("记忆分流响应无效")
}
