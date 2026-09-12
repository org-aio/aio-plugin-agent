use super::model::*;
use sqlx::{PgPool, Row, postgres::PgRow};
use uuid::Uuid;

pub fn provider(row: PgRow) -> Provider {
    Provider {
        id: row.get("id"),
        label: row.get("label"),
        endpoint: row.get("endpoint"),
        model: row.get("model"),
        has_secret: row.get("has_secret"),
    }
}
pub fn conversation(row: PgRow) -> Conversation {
    Conversation {
        id: row.get("id"),
        title: row.get("title"),
        provider_id: row.get("provider_id"),
        space_id: row.get("space_id"),
        updated_at: row.get("updated_at"),
    }
}
pub const CONVERSATION_COLUMNS: &str =
    "id, title, provider_id, space_id, updated_at::text AS updated_at";

// 调用方持有任务锁且确认没有活跃任务，避免把正常生成误判为中断。
pub async fn recover(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agent_messages SET status='interrupted', error='生成任务已结束，最终保存未确认；保留最近已保存内容' WHERE conversation_id=$1 AND status='generating'")
        .bind(id).execute(pool).await?;
    Ok(())
}

pub async fn finish(
    pool: &PgPool,
    conversation: Uuid,
    assistant: Uuid,
    content: &str,
    status: &str,
    failure: Option<&str>,
    tokens: Option<i64>,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE agent_messages SET content=$1,status=$2,error=$3,tokens=$4 WHERE id=$5")
        .bind(content)
        .bind(status)
        .bind(failure)
        .bind(tokens)
        .bind(assistant)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE agent_conversations SET updated_at=now() WHERE id=$1")
        .bind(conversation)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

pub async fn owned(pool: &PgPool, scope: &Scope, id: Uuid) -> ServiceResult<Conversation> {
    let row = sqlx::query(&format!("SELECT {CONVERSATION_COLUMNS} FROM agent_conversations WHERE id=$1 AND tenant_id=$2 AND user_id=$3"))
        .bind(id).bind(&scope.tenant).bind(&scope.user).fetch_optional(pool).await?.ok_or_else(missing)?;
    Ok(conversation(row))
}
pub async fn thread(pool: &PgPool, scope: &Scope, id: Uuid) -> ServiceResult<Thread> {
    let conversation = owned(pool, scope, id).await?;
    let rows = sqlx::query("SELECT id, role, content, status, error, tokens,source_id,memory_status,citations FROM agent_messages WHERE conversation_id=$1 ORDER BY sequence LIMIT 400")
        .bind(id).fetch_all(pool).await?;
    Ok(Thread {
        conversation,
        messages: rows
            .into_iter()
            .map(|r| Message {
                id: r.get("id"),
                role: r.get("role"),
                content: r.get("content"),
                status: r.get("status"),
                error: r.get("error"),
                tokens: r.get("tokens"),
                source_id: r.get("source_id"),
                memory_status: r.get("memory_status"),
                citations: serde_json::from_value(r.get("citations")).unwrap_or_default(),
            })
            .collect(),
    })
}
