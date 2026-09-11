use super::{
    model::*,
    provider::{self, Delta},
    service_impl::Core,
    store, util,
};
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub async fn start(
    core: Arc<Core>,
    scope: &Scope,
    id: Uuid,
    prompt: Prompt,
) -> ServiceResult<Thread> {
    let content = util::text(&prompt.content, 16000, "消息")?;
    let mut jobs = core.jobs.lock().await;
    if core.shutdown.is_cancelled() {
        return Err(conflict("服务正在停止"));
    }
    store::owned(&core.pool, scope, id).await?;
    if !jobs.contains_key(&id) {
        store::recover(&core.pool, id).await?;
    }
    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_messages WHERE conversation_id=$1 AND request_id=$2)",
    )
    .bind(id)
    .bind(prompt.request_id)
    .fetch_one(&core.pool)
    .await?;
    if duplicate {
        return store::thread(&core.pool, scope, id).await;
    }
    if jobs.contains_key(&id) {
        return Err(conflict("此会话正在生成"));
    }
    let permit = core
        .quota
        .clone()
        .try_acquire_owned()
        .map_err(|_| conflict("生成并发已满，请稍后重试"))?;
    let mut tx = core.pool.begin().await?;
    let row = sqlx::query("SELECT p.id,p.endpoint,p.model,p.secret FROM agent_providers p JOIN agent_conversations c ON c.provider_id=p.id WHERE c.id=$1 AND c.tenant_id=$2 AND c.user_id=$3 FOR UPDATE OF c")
        .bind(id).bind(&scope.tenant).bind(&scope.user).fetch_optional(&mut *tx).await?.ok_or_else(missing)?;
    let endpoint: String = row.get("endpoint");
    let model: String = row.get("model");
    core.config
        .endpoint(&endpoint)
        .map_err(|e| bad(&e.to_string()))?;
    let secret = row
        .get::<Option<Vec<u8>>, _>("secret")
        .map(|v| {
            util::decrypt(
                &core.config.encryption_key,
                &v,
                &util::owner(scope, row.get("id")),
            )
        })
        .transpose()?;
    let history = sqlx::query(
        "SELECT role,content,status FROM agent_messages WHERE conversation_id=$1 ORDER BY sequence",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    if history.len() >= 398 {
        return Err(bad("会话已达 200 轮，请新建会话"));
    }
    let mut messages: Vec<serde_json::Value> = history
        .iter()
        .filter(|r| r.get::<String, _>("status") == "complete")
        .map(|r| json!({"role":r.get::<String,_>("role"),"content":r.get::<String,_>("content")}))
        .collect();
    messages.push(json!({"role":"user","content":content}));
    if serde_json::to_vec(&messages)
        .map_err(anyhow::Error::from)?
        .len()
        > 160000
    {
        return Err(bad("会话上下文过长，请新建会话"));
    }
    let assistant = Uuid::new_v4();
    for (message_id, role, body, status) in [
        (Uuid::new_v4(), "user", content.as_str(), "complete"),
        (assistant, "assistant", "", "generating"),
    ] {
        sqlx::query("INSERT INTO agent_messages(id,conversation_id,request_id,role,content,status) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(message_id).bind(id).bind(prompt.request_id).bind(role).bind(body).bind(status).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE agent_conversations SET updated_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let cancel = core.shutdown.child_token();
    jobs.insert(id, cancel.clone());
    let task_core = core.clone();
    tokio::spawn(async move {
        run(
            task_core.clone(),
            id,
            assistant,
            endpoint,
            model,
            secret,
            messages,
            cancel,
            permit,
        )
        .await;
        task_core.jobs.lock().await.remove(&id);
    });
    drop(jobs);
    store::thread(&core.pool, scope, id).await
}

#[allow(clippy::too_many_arguments)]
async fn run(
    core: Arc<Core>,
    conversation: Uuid,
    assistant: Uuid,
    endpoint: String,
    model: String,
    secret: Option<String>,
    messages: Vec<serde_json::Value>,
    cancel: CancellationToken,
    _permit: OwnedSemaphorePermit,
) {
    let (sender, mut receiver) = mpsc::channel(32);
    let client = core.client.clone();
    let timeout = core.config.generation_timeout;
    let upstream = tokio::spawn(async move {
        tokio::time::timeout(
            timeout,
            provider::generate(
                &client,
                &endpoint,
                &model,
                secret.as_deref(),
                messages,
                sender,
            ),
        )
        .await
    });
    let mut content = String::new();
    let mut tokens = None;
    let mut cancelled = false;
    let mut failure = None;
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(250));
    let mut dirty = false;
    loop {
        tokio::select! {
            biased;
            _=cancel.cancelled()=>{cancelled=true;upstream.abort();break;}
            delta=receiver.recv()=>match delta {
                Some(Delta::Text(text))=>{ content.push_str(&text);dirty=true; if content.len()>128000 {failure=Some("回复超过长度上限".to_owned());upstream.abort();break;} }
                Some(Delta::Tokens(value))=>{tokens=Some(value);dirty=true;}
                None=>break,
            },
            _=ticker.tick(), if dirty=> {
                if sqlx::query("UPDATE agent_messages SET content=$1,tokens=$2 WHERE id=$3").bind(&content).bind(tokens).bind(assistant).execute(&core.pool).await.is_err() {
                    failure=Some("保存回复失败".into());upstream.abort();break;
                }
                dirty=false;
            }
        }
    }
    if !cancelled && failure.is_none() {
        failure = match upstream.await {
            Ok(Ok(Ok(()))) => None,
            Ok(Ok(Err(e))) => Some(e.to_string()),
            Ok(Err(_)) => Some("模型生成超时".into()),
            Err(_) => Some("模型生成任务中断".into()),
        };
    } else {
        let _ = upstream.await;
    }
    let status = if cancelled {
        "cancelled"
    } else if failure.is_some() {
        "failed"
    } else {
        "complete"
    };
    if store::finish(
        &core.pool,
        conversation,
        assistant,
        &content,
        status,
        failure.as_deref(),
        tokens,
    )
    .await
    .is_err()
    {
        eprintln!("会话 {conversation} 最终保存失败，后续读取将恢复中断状态");
    }
}
