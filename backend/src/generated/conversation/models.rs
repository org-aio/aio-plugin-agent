use super::{model::*, service_impl::Core, store, util};
use futures_util::StreamExt;
use sqlx::Row;
use std::time::Duration;
use uuid::Uuid;

pub(super) async fn list(
    core: &Core,
    scope: &Scope,
    request: ModelListRequest,
) -> ServiceResult<Vec<String>> {
    let endpoint = request.endpoint.trim().trim_end_matches('/');
    core.config
        .endpoint(endpoint)
        .map_err(|e| bad(&e.to_string()))?;
    let saved = if let Some(id) = request.provider_id {
        Some((id, sqlx::query("SELECT endpoint,secret FROM agent_providers WHERE id=$1 AND tenant_id=$2 AND user_id=$3")
            .bind(id).bind(&scope.tenant).bind(&scope.user).fetch_optional(&core.pool).await?.ok_or_else(missing)?))
    } else {
        None
    };
    let secret = match request.secret {
        Some(secret) => {
            if secret.len() > 8192 || secret.contains(['\r', '\n']) {
                return Err(bad("密钥格式无效"));
            }
            Some(secret).filter(|value| !value.is_empty())
        }
        None => match saved {
            Some((id, row)) if row.get::<String, _>("endpoint") == endpoint => row
                .get::<Option<Vec<u8>>, _>("secret")
                .map(|value| {
                    util::decrypt(&core.config.encryption_key, &value, &util::owner(scope, id))
                })
                .transpose()?,
            _ => None,
        },
    };
    let _permit = core
        .quota
        .clone()
        .try_acquire_owned()
        .map_err(|_| conflict("模型请求并发已满"))?;
    let mut http = if let Some(gateway) = &core.config.gateway {
        core.client
            .get("http://localhost/egress/models")
            .header("x-aio-endpoint", endpoint)
            .header("x-aio-token", &gateway.token)
    } else {
        core.client.get(format!("{endpoint}/models"))
    };
    if let Some(secret) = secret {
        http = http.bearer_auth(secret);
    }
    let response = http
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|_| bad("模型列表连接失败"))?;
    if !response.status().is_success() {
        return Err(bad(&format!(
            "模型列表返回 HTTP {}",
            response.status().as_u16()
        )));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| bad("读取模型列表失败"))?;
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err(bad("模型列表超过配额"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let catalog: ModelCatalog =
        serde_json::from_slice(&bytes).map_err(|_| bad("模型列表格式无效"))?;
    let mut models: Vec<_> = catalog
        .data
        .into_iter()
        .map(|model| model.id)
        .filter(|id| !id.trim().is_empty() && id.chars().count() <= 160)
        .collect();
    models.sort();
    models.dedup();
    Ok(models)
}

pub(super) async fn select(
    core: &Core,
    scope: &Scope,
    id: Uuid,
    selection: ModelSelection,
) -> ServiceResult<Conversation> {
    let mut tx = core.pool.begin().await?;
    let owned: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM agent_conversations WHERE id=$1 AND tenant_id=$2 AND user_id=$3 FOR UPDATE",
    )
    .bind(id)
    .bind(&scope.tenant)
    .bind(&scope.user)
    .fetch_optional(&mut *tx)
    .await?;
    owned.ok_or_else(missing)?;
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_messages WHERE conversation_id=$1 AND status IN ('queued','generating'))")
        .bind(id).fetch_one(&mut *tx).await?;
    if active {
        return Err(conflict("请等待当前回复完成或停止生成后切换模型"));
    }
    if let Some(provider) = selection.provider_id {
        let owned: Option<Uuid> = sqlx::query_scalar("SELECT id FROM agent_providers WHERE id=$1 AND tenant_id=$2 AND user_id=$3 FOR KEY SHARE")
            .bind(provider).bind(&scope.tenant).bind(&scope.user).fetch_optional(&mut *tx).await?;
        owned.ok_or_else(missing)?;
    }
    let row = sqlx::query(&format!(
        "UPDATE agent_conversations SET provider_id=$1,updated_at=now() WHERE id=$2 RETURNING {}",
        store::CONVERSATION_COLUMNS
    ))
    .bind(selection.provider_id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(store::conversation(row))
}
