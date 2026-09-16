use crate::generated::conversation::{model::*, service_impl::Core, util};
use sqlx::Row;

pub(super) fn owner(scope: &Scope) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&(
        &scope.tenant,
        &scope.user,
        "web-search",
    ))?)
}

pub(crate) async fn read(core: &Core, scope: &Scope) -> ServiceResult<SearchSettings> {
    let row = sqlx::query("SELECT enabled,secret IS NOT NULL AS has_secret FROM agent_tool_settings WHERE tenant_id=$1 AND user_id=$2 AND tool='web_search'")
        .bind(&scope.tenant).bind(&scope.user).fetch_optional(&core.pool).await?;
    Ok(row
        .map(|row| SearchSettings {
            enabled: row.get("enabled"),
            has_secret: row.get("has_secret"),
        })
        .unwrap_or_default())
}

pub(crate) async fn save(
    core: &Core,
    scope: &Scope,
    draft: SearchSettingsDraft,
) -> ServiceResult<SearchSettings> {
    if draft
        .secret
        .as_ref()
        .is_some_and(|value| value.len() > 8192 || value.contains(['\r', '\n']))
    {
        return Err(bad("密钥格式无效"));
    }
    let mut tx = core.pool.begin().await?;
    // 首次配置与后续更新使用同一行锁，避免保留密钥的更新覆盖并发修改。
    sqlx::query("INSERT INTO agent_tool_settings(tenant_id,user_id,tool) VALUES($1,$2,'web_search') ON CONFLICT DO NOTHING")
        .bind(&scope.tenant).bind(&scope.user).execute(&mut *tx).await?;
    let current: Option<Vec<u8>> = sqlx::query_scalar("SELECT secret FROM agent_tool_settings WHERE tenant_id=$1 AND user_id=$2 AND tool='web_search' FOR UPDATE")
        .bind(&scope.tenant).bind(&scope.user).fetch_one(&mut *tx).await?;
    let secret = match draft.secret {
        None => current,
        Some(value) if value.is_empty() => None,
        Some(value) => Some(util::encrypt(
            &core.config.encryption_key,
            &value,
            &owner(scope)?,
        )?),
    };
    if draft.enabled && secret.is_none() {
        return Err(bad("启用网页搜索需要填写 Tavily API Key"));
    }
    sqlx::query("UPDATE agent_tool_settings SET enabled=$3,secret=$4 WHERE tenant_id=$1 AND user_id=$2 AND tool='web_search'")
        .bind(&scope.tenant).bind(&scope.user).bind(draft.enabled).bind(&secret).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(SearchSettings {
        enabled: draft.enabled,
        has_secret: secret.is_some(),
    })
}
