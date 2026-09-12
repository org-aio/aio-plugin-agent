use super::{model::*, service::AgentService, store, util};
use crate::configuration::RuntimeConfig;
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub struct AgentServiceImpl {
    pub(super) core: Arc<Core>,
}
pub(crate) struct Core {
    pub pool: PgPool,
    pub config: RuntimeConfig,
    pub client: reqwest::Client,
    pub jobs: Mutex<HashMap<Uuid, CancellationToken>>,
    pub quota: Arc<Semaphore>,
    pub shutdown: CancellationToken,
    pub _lease: sqlx::pool::PoolConnection<sqlx::Postgres>,
    pub worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl AgentServiceImpl {
    pub async fn connect(config: RuntimeConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::query("SET statement_timeout = '3s'")
                        .execute(&mut *connection)
                        .await?;
                    sqlx::query("SET idle_in_transaction_session_timeout = '5s'")
                        .execute(&mut *connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&config.database_url)
            .await
            .context("连接 Agent 数据库失败")?;
        let privileged: bool = sqlx::query_scalar("SELECT rolsuper OR rolcreaterole OR rolcreatedb OR rolbypassrls FROM pg_roles WHERE rolname=current_user").fetch_one(&pool).await?;
        ensure!(!privileged, "Agent 不能使用数据库管理员凭据");
        let mut lease = pool.acquire().await?;
        let locked: bool = sqlx::query_scalar(
            "SELECT pg_try_advisory_lock(hashtextextended(current_schema() || ':agent-worker',0))",
        )
        .fetch_one(&mut *lease)
        .await?;
        ensure!(locked, "同一数据空间已有 Agent 实例运行，请先排空旧实例");
        sqlx::query("UPDATE agent_messages SET status='interrupted', error='服务重启，生成已中断' WHERE status='generating'").execute(&pool).await?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .build()?;
        let core = Arc::new(Core {
            pool,
            config,
            client,
            jobs: Mutex::new(HashMap::new()),
            quota: Arc::new(Semaphore::new(4)),
            shutdown: CancellationToken::new(),
            _lease: lease,
            worker: Mutex::new(None),
        });
        super::intake::protect_history(&core).await?;
        *core.worker.lock().await = Some(super::worker::start(Arc::downgrade(&core)));
        Ok(Self { core })
    }
}

#[async_trait]
impl AgentService for AgentServiceImpl {
    async fn settings(&self, scope: &Scope) -> ServiceResult<Settings> {
        let providers = sqlx::query("SELECT id,label,endpoint,model,secret IS NOT NULL AS has_secret FROM agent_providers WHERE tenant_id=$1 AND user_id=$2 ORDER BY label")
            .bind(&scope.tenant).bind(&scope.user).fetch_all(&self.core.pool).await?.into_iter().map(store::provider).collect();
        Ok(Settings {
            allowed_endpoints: self.core.config.allowed_endpoints.iter().cloned().collect(),
            providers,
            max_prompt_chars: 16000,
            memory_available: self.core.config.memory.is_some(),
        })
    }
    async fn save_provider(
        &self,
        scope: &Scope,
        id: Option<Uuid>,
        draft: ProviderDraft,
    ) -> ServiceResult<Provider> {
        let label = util::text(&draft.label, 80, "名称")?;
        let model = util::text(&draft.model, 160, "模型")?;
        self.core
            .config
            .endpoint(&draft.endpoint)
            .map_err(|e| bad(&e.to_string()))?;
        let endpoint = draft.endpoint.trim_end_matches('/');
        let existing = id;
        let id = id.unwrap_or_else(Uuid::new_v4);
        let secret = draft
            .secret
            .map(|s| {
                if s.is_empty() {
                    return Ok(None);
                }
                if s.len() > 8192 || s.contains(['\r', '\n']) {
                    return Err(bad("密钥格式无效"));
                }
                Ok(Some(util::encrypt(
                    &self.core.config.encryption_key,
                    &s,
                    &util::owner(scope, id),
                )?))
            })
            .transpose()?;
        if existing.is_some() {
            let mut tx = self.core.pool.begin().await?;
            let old = sqlx::query("SELECT endpoint,secret FROM agent_providers WHERE id=$1 AND tenant_id=$2 AND user_id=$3 FOR UPDATE")
                .bind(id).bind(&scope.tenant).bind(&scope.user).fetch_optional(&mut *tx).await?.ok_or_else(missing)?;
            // 地址变化必须重新输入凭据，不能把已有密钥发到新地址。
            let secret = match secret {
                Some(value) => value,
                None if old.get::<String, _>("endpoint") == endpoint => old.get("secret"),
                None => None,
            };
            sqlx::query(
                "UPDATE agent_providers SET label=$1,endpoint=$2,model=$3,secret=$4 WHERE id=$5",
            )
            .bind(&label)
            .bind(endpoint)
            .bind(&model)
            .bind(&secret)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(Provider {
                id,
                label,
                model,
                endpoint: endpoint.into(),
                has_secret: secret.is_some(),
            })
        } else {
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM agent_providers WHERE tenant_id=$1 AND user_id=$2",
            )
            .bind(&scope.tenant)
            .bind(&scope.user)
            .fetch_one(&self.core.pool)
            .await?;
            if count >= 24 {
                return Err(bad("最多配置 24 个模型"));
            }
            let secret = secret.flatten();
            sqlx::query("INSERT INTO agent_providers(id,tenant_id,user_id,label,endpoint,model,secret) VALUES($1,$2,$3,$4,$5,$6,$7)")
                .bind(id).bind(&scope.tenant).bind(&scope.user).bind(&label).bind(endpoint).bind(&model).bind(&secret).execute(&self.core.pool).await?;
            Ok(Provider {
                id,
                label,
                model,
                endpoint: endpoint.into(),
                has_secret: secret.is_some(),
            })
        }
    }
    async fn delete_provider(&self, scope: &Scope, id: Uuid) -> ServiceResult<()> {
        let result =
            sqlx::query("DELETE FROM agent_providers WHERE id=$1 AND tenant_id=$2 AND user_id=$3")
                .bind(id)
                .bind(&scope.tenant)
                .bind(&scope.user)
                .execute(&self.core.pool)
                .await;
        match result {
            Err(sqlx::Error::Database(e)) if e.is_foreign_key_violation() => {
                Err(conflict("该模型仍被会话使用，请先删除相关会话"))
            }
            Err(e) => Err(e.into()),
            Ok(r) if r.rows_affected() == 0 => Err(missing()),
            Ok(_) => Ok(()),
        }
    }
    async fn conversations(&self, scope: &Scope) -> ServiceResult<Vec<Conversation>> {
        Ok(sqlx::query(&format!("SELECT {} FROM agent_conversations WHERE tenant_id=$1 AND user_id=$2 ORDER BY updated_at DESC LIMIT 200",store::CONVERSATION_COLUMNS))
            .bind(&scope.tenant).bind(&scope.user).fetch_all(&self.core.pool).await?.into_iter().map(store::conversation).collect())
    }
    async fn create(&self, scope: &Scope, draft: ConversationDraft) -> ServiceResult<Conversation> {
        let title = util::text(&draft.title, 160, "会话标题")?;
        let id = Uuid::new_v4();
        if let Some(provider) = draft.provider_id {
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_providers WHERE id=$1 AND tenant_id=$2 AND user_id=$3)").bind(provider).bind(&scope.tenant).bind(&scope.user).fetch_one(&self.core.pool).await?;
            if !exists {
                return Err(missing());
            }
        }
        if let Some(space) = &draft.space_id {
            if space.len() != 32 || !space.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(bad("记忆空间无效"));
            }
            let spaces = super::memory::invoke(
                &self.core,
                scope,
                "GET",
                "/spaces",
                serde_json::Value::Null,
                false,
            )
            .await?;
            if !spaces.as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item["id"] == *space && item["role"] != "READER")
            }) {
                return Err(missing());
            }
        }
        let row = sqlx::query(&format!("INSERT INTO agent_conversations(id,tenant_id,user_id,title,provider_id,space_id) VALUES($1,$2,$3,$4,$5,$6) RETURNING {}",store::CONVERSATION_COLUMNS))
            .bind(id).bind(&scope.tenant).bind(&scope.user).bind(title).bind(draft.provider_id).bind(draft.space_id).fetch_one(&self.core.pool).await?;
        Ok(store::conversation(row))
    }
    async fn thread(&self, scope: &Scope, id: Uuid) -> ServiceResult<Thread> {
        store::owned(&self.core.pool, scope, id).await?;
        let jobs = self.core.jobs.lock().await;
        if !jobs.contains_key(&id) {
            store::recover(&self.core.pool, id).await?;
        }
        drop(jobs);
        super::memory::safe_thread(&self.core, scope, id, false).await
    }
    async fn delete(&self, scope: &Scope, id: Uuid) -> ServiceResult<()> {
        store::owned(&self.core.pool, scope, id).await?;
        let jobs = self.core.jobs.lock().await;
        if jobs.contains_key(&id) {
            return Err(conflict("请先停止生成"));
        }
        sqlx::query("DELETE FROM agent_conversations WHERE id=$1 AND tenant_id=$2 AND user_id=$3")
            .bind(id)
            .bind(&scope.tenant)
            .bind(&scope.user)
            .execute(&self.core.pool)
            .await?;
        Ok(())
    }
    async fn send(&self, scope: &Scope, id: Uuid, prompt: Prompt) -> ServiceResult<Thread> {
        super::intake::receive(self.core.clone(), scope, id, prompt).await
    }
    async fn cancel(&self, scope: &Scope, id: Uuid) -> ServiceResult<Thread> {
        store::owned(&self.core.pool, scope, id).await?;
        if let Some(token) = self.core.jobs.lock().await.get(&id) {
            token.cancel();
        }
        self.thread(scope, id).await
    }
    async fn memory_request(
        &self,
        scope: &Scope,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> ServiceResult<serde_json::Value> {
        let root = path
            .trim_start_matches('/')
            .split(['/', '?'])
            .next()
            .unwrap_or("");
        if !matches!(method, "GET" | "POST" | "PUT" | "DELETE")
            || !matches!(
                root,
                "spaces"
                    | "sources"
                    | "secrets"
                    | "nodes"
                    | "graph"
                    | "search"
                    | "context"
                    | "capture"
                    | "import"
            )
            || path.contains("..")
            || path.len() > 2048
        {
            return Err(bad("记忆请求无效"));
        }
        let model = if root == "spaces" && matches!(method, "PUT" | "POST") {
            body.get("modelBinding")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
        } else {
            None
        };
        let model = model
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| bad("模型绑定无效"))?;
        if let Some(id) = model {
            let owned: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_providers WHERE id=$1 AND tenant_id=$2 AND user_id=$3)").bind(id).bind(&scope.tenant).bind(&scope.user).fetch_one(&self.core.pool).await?;
            if !owned {
                return Err(missing());
            }
        }
        let result = super::memory::invoke(&self.core, scope, method, path, body, true).await?;
        if let (Some(provider), Some(space)) = (model, result["id"].as_str()) {
            sqlx::query("INSERT INTO agent_model_grants(provider_id,space_id) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(provider).bind(space).execute(&self.core.pool).await?;
        }
        Ok(result)
    }
    async fn shutdown(&self) {
        self.core.shutdown.cancel();
        if let Some(worker) = self.core.worker.lock().await.take() {
            let _ = worker.await;
        }
        for _ in 0..100 {
            if self.core.jobs.lock().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
