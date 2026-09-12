use super::{memory, model::*, service_impl::Core, util};
use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

pub async fn conversation(
    core: &Core,
    scope: &Scope,
    provider: Option<Uuid>,
    space: &str,
) -> Result<Option<ModelConnection>> {
    if let Some(id) = provider {
        let row = sqlx::query("SELECT id,endpoint,model,secret,user_id FROM agent_providers WHERE id=$1 AND tenant_id=$2 AND user_id=$3")
            .bind(id).bind(&scope.tenant).bind(&scope.user).fetch_one(&core.pool).await?;
        return decode(core, scope, row).map(Some);
    }
    let spaces = memory::invoke(core, scope, "GET", "/spaces", Value::Null, false).await?;
    let binding = spaces
        .as_array()
        .context("空间响应无效")?
        .iter()
        .find(|item| item["id"] == space)
        .context("空间不可访问")?["modelBinding"]
        .as_str()
        .filter(|value| !value.is_empty());
    let Some(binding) = binding else {
        return Ok(None);
    };
    shared(core, scope, Uuid::parse_str(binding)?, space)
        .await
        .map(Some)
}

pub async fn shared(core: &Core, scope: &Scope, id: Uuid, space: &str) -> Result<ModelConnection> {
    let row = sqlx::query("SELECT p.id,p.endpoint,p.model,p.secret,p.user_id FROM agent_providers p JOIN agent_model_grants g ON g.provider_id=p.id WHERE p.id=$1 AND p.tenant_id=$2 AND g.space_id=$3")
        .bind(id).bind(&scope.tenant).bind(space).fetch_optional(&core.pool).await?
        .context("模型未授权给该空间")?;
    decode(core, scope, row)
}

fn decode(core: &Core, scope: &Scope, row: PgRow) -> Result<ModelConnection> {
    let endpoint: String = row.get("endpoint");
    core.config.endpoint(&endpoint)?;
    let owner = Scope {
        context_id: None,
        tenant: scope.tenant.clone(),
        user: row.get("user_id"),
    };
    let secret = row
        .get::<Option<Vec<u8>>, _>("secret")
        .map(|value| {
            util::decrypt(
                &core.config.encryption_key,
                &value,
                &util::owner(&owner, row.get("id")),
            )
        })
        .transpose()?;
    Ok(ModelConnection {
        endpoint,
        model: row.get("model"),
        secret,
    })
}
