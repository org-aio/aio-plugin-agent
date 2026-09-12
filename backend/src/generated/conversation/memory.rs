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
            message.memory_status = Some("unavailable".into());
            message.error = None;
        }
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
    let response = core.client.post(connection.endpoint.clone()).bearer_auth(&connection.token)
        .json(&json!({"method":method,"path":path,"body":body,"tenantId":scope.tenant,"userId":scope.user,"interactive":interactive}))
        .timeout(std::time::Duration::from_secs(30)).send().await.context("记忆服务暂不可用")?;
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

pub async fn context(
    core: &Core,
    scope: &Scope,
    space: &str,
    query: &str,
) -> Result<(String, Vec<super::model::MemoryCitation>)> {
    let graph = invoke(
        core,
        scope,
        "POST",
        &format!("/recall?spaceId={space}"),
        json!({"query":query,"limit":8}),
        false,
    )
    .await?;
    let nodes: Vec<_> = graph["nodes"].as_array().cloned().unwrap_or_default();
    let ids: Vec<_> = nodes
        .iter()
        .take(8)
        .filter_map(|node| node["id"].as_str())
        .collect();
    if ids.is_empty() {
        return Ok((String::new(), Vec::new()));
    }
    let result = invoke(
        core,
        scope,
        "POST",
        &format!("/context?spaceId={space}"),
        json!({"nodeIds":ids,"depth":1,"maxCharacters":12000}),
        false,
    )
    .await?;
    let citations = result["nodeIds"]
        .as_array()
        .context("上下文来源无效")?
        .iter()
        .filter_map(|value| {
            let id = value.as_str()?;
            Some(super::model::MemoryCitation {
                id: id.into(),
                title: nodes
                    .iter()
                    .find(|node| node["id"] == id)
                    .and_then(|node| node["title"].as_str())
                    .unwrap_or("关联资料")
                    .into(),
            })
        })
        .collect();
    Ok((
        result["markdown"].as_str().unwrap_or("").to_owned(),
        citations,
    ))
}
