use super::{model::Request, service::SkillService, util};
use crate::generated::conversation::{
    model::{Scope, ServiceResult, bad, conflict, missing},
    service_impl::{AgentServiceImpl, Core},
    util::{decrypt, encrypt},
};
use az_agent_model::{SkillContent, SkillDevice, SkillFile, SkillLibrary};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sqlx::Row;

#[async_trait::async_trait]
impl SkillService for AgentServiceImpl {
    async fn skill_request(
        &self,
        scope: &Scope,
        request: Request,
        worker: bool,
    ) -> ServiceResult<Value> {
        let device = if worker {
            let id = scope
                .context_id
                .as_deref()
                .and_then(|s| s.strip_prefix("worker:"))
                .ok_or_else(|| bad("需要已配对设备身份"))?;
            uuid::Uuid::parse_str(id).map_err(|_| bad("设备身份无效"))?;
            Some(id)
        } else {
            None
        };
        match request {
            Request::List => list(&self.core, scope).await,
            Request::Read { path } => read(&self.core, scope, &path).await,
            Request::Write {
                path,
                expected,
                content,
                executable,
            } => write(&self.core, scope, &path, expected, content, executable).await,
            Request::Status { report } => {
                let device = device.ok_or_else(|| bad("仅设备可以提交同步状态"))?;
                if !report.is_object()
                    || serde_json::to_vec(&report)
                        .map_err(anyhow::Error::from)?
                        .len()
                        > 128 * 1024
                {
                    return Err(bad("同步状态过大或格式无效"));
                }
                sqlx::query("INSERT INTO agent_skill_devices(tenant_id,user_id,device_id,report) VALUES($1,$2,$3,$4) ON CONFLICT(tenant_id,user_id,device_id) DO UPDATE SET report=$4,updated_at=now(),resolutions=CASE WHEN $4->>'phase'='complete' THEN '{}'::jsonb ELSE agent_skill_devices.resolutions END")
                    .bind(&scope.tenant).bind(&scope.user).bind(device).bind(report).execute(&self.core.pool).await?;
                Ok(Value::Null)
            }
            Request::Resolve {
                device: target,
                path,
                side,
                local,
                remote,
            } => {
                if worker || !matches!(side.as_str(), "local" | "remote") {
                    return Err(bad("冲突处理选项无效"));
                }
                util::validate_path(&path)?;
                let mut tx = self.core.pool.begin().await?;
                let report: Value = sqlx::query_scalar("SELECT report FROM agent_skill_devices WHERE tenant_id=$1 AND user_id=$2 AND device_id=$3 FOR UPDATE")
                    .bind(&scope.tenant).bind(&scope.user).bind(&target).fetch_optional(&mut *tx).await?.ok_or_else(missing)?;
                let current = report["conflicts"]
                    .as_array()
                    .and_then(|v| v.iter().find(|c| c["path"] == path))
                    .ok_or_else(|| conflict("冲突已变化，请刷新"))?;
                if current["local"] != json!(local) || current["remote"] != json!(remote) {
                    return Err(conflict("冲突版本已变化"));
                }
                let resolution = json!({"side":side,"local":local,"remote":remote});
                sqlx::query("UPDATE agent_skill_devices SET resolutions=jsonb_set(resolutions,ARRAY[$4]::text[],$5,true) WHERE tenant_id=$1 AND user_id=$2 AND device_id=$3")
                    .bind(&scope.tenant).bind(&scope.user).bind(target).bind(path).bind(resolution).execute(&mut *tx).await?;
                tx.commit().await?;
                Ok(Value::Null)
            }
        }
    }
}

fn file(row: &sqlx::postgres::PgRow) -> SkillFile {
    SkillFile {
        path: row.get("path"),
        hash: row.get("hash"),
        executable: row.get("executable"),
        size: row.get("size"),
    }
}
pub(super) async fn list(core: &Core, scope: &Scope) -> ServiceResult<Value> {
    let files = sqlx::query("SELECT path,hash,executable,size FROM agent_skill_files WHERE tenant_id=$1 AND user_id=$2 ORDER BY path")
        .bind(&scope.tenant).bind(&scope.user).fetch_all(&core.pool).await?.iter().map(file).collect();
    let devices = sqlx::query("SELECT device_id,report,resolutions,(extract(epoch FROM updated_at)*1000)::bigint AS updated FROM agent_skill_devices WHERE tenant_id=$1 AND user_id=$2 ORDER BY updated_at DESC")
        .bind(&scope.tenant).bind(&scope.user).fetch_all(&core.pool).await?.into_iter().map(|r| SkillDevice { id:r.get("device_id"), report:r.get("report"), resolutions:r.get("resolutions"), updated_at:r.get("updated") }).collect();
    Ok(serde_json::to_value(SkillLibrary { files, devices }).map_err(anyhow::Error::from)?)
}
pub(super) async fn read(core: &Core, scope: &Scope, path: &str) -> ServiceResult<Value> {
    util::validate_path(path)?;
    let row = sqlx::query(
        "SELECT * FROM agent_skill_files WHERE tenant_id=$1 AND user_id=$2 AND path=$3",
    )
    .bind(&scope.tenant)
    .bind(&scope.user)
    .bind(path)
    .fetch_optional(&core.pool)
    .await?
    .ok_or_else(missing)?;
    let encrypted: Option<Vec<u8>> = row.get("content");
    let content = encrypted
        .map(|bytes| {
            decrypt(
                &core.config.encryption_key,
                &bytes,
                &util::owner(scope, path),
            )
        })
        .transpose()?;
    Ok(serde_json::to_value(SkillContent {
        file: file(&row),
        content,
    })
    .map_err(anyhow::Error::from)?)
}
async fn write(
    core: &Core,
    scope: &Scope,
    path: &str,
    expected: Option<String>,
    content: Option<String>,
    executable: bool,
) -> ServiceResult<Value> {
    util::validate_path(path)?;
    let bytes = content
        .as_ref()
        .map(|s| STANDARD.decode(s).map_err(|_| bad("文件编码无效")))
        .transpose()?;
    let size = bytes.as_ref().map_or(0, Vec::len) as i64;
    if size > 2 * 1024 * 1024 {
        return Err(bad("单文件不能超过 2 MiB"));
    }
    let hash = bytes.as_ref().map(|b| util::hash(b, executable));
    let encrypted = content
        .as_ref()
        .map(|s| encrypt(&core.config.encryption_key, s, &util::owner(scope, path)))
        .transpose()?;
    let mut tx = core.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("skills:{}:{}", scope.tenant, scope.user))
        .execute(&mut *tx)
        .await?;
    let old = sqlx::query("SELECT hash,content,executable,size FROM agent_skill_files WHERE tenant_id=$1 AND user_id=$2 AND path=$3")
        .bind(&scope.tenant).bind(&scope.user).bind(path).fetch_optional(&mut *tx).await?;
    let current: Option<String> = old.as_ref().and_then(|r| r.get("hash"));
    if current == hash {
        return Ok(json!({"path":path,"hash":hash,"size":size,"executable":executable}));
    }
    if current != expected {
        return Err(conflict("文件已被修改，请刷新后处理冲突"));
    }
    let (total, count): (i64,i64) = sqlx::query_as("SELECT coalesce(sum(size),0)::bigint,count(*) FROM agent_skill_files WHERE tenant_id=$1 AND user_id=$2")
        .bind(&scope.tenant).bind(&scope.user).fetch_one(&mut *tx).await?;
    if total - old.as_ref().map_or(0, |r| r.get::<i64, _>("size")) + size > 32 * 1024 * 1024
        || (old.is_none() && count >= 4096)
    {
        return Err(bad("个人 Skill 库已达到容量限制"));
    }
    if let Some(old) = &old {
        sqlx::query("INSERT INTO agent_skill_history(tenant_id,user_id,path,hash,content,executable) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(&scope.tenant).bind(&scope.user).bind(path).bind(&current).bind(old.get::<Option<Vec<u8>>,_>("content")).bind(old.get::<bool,_>("executable")).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO agent_skill_files(tenant_id,user_id,path,hash,content,executable,size) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(tenant_id,user_id,path) DO UPDATE SET hash=$4,content=$5,executable=$6,size=$7,updated_at=now()")
        .bind(&scope.tenant).bind(&scope.user).bind(path).bind(&hash).bind(encrypted).bind(executable).bind(size).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({"path":path,"hash":hash,"size":size,"executable":executable}))
}
