use super::{model::*, service_impl::Core, store, util};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use uuid::Uuid;

fn fingerprint(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

pub async fn protect_history(core: &Core) -> anyhow::Result<()> {
    loop {
        let mut tx = core.pool.begin().await?;
        let rows = sqlx::query("SELECT m.id,m.content,m.role,m.conversation_id,m.request_id,c.tenant_id,c.user_id FROM agent_messages m JOIN agent_conversations c ON c.id=m.conversation_id WHERE m.memory_status IS NULL ORDER BY m.sequence LIMIT 100 FOR UPDATE OF m").fetch_all(&mut *tx).await?;
        if rows.is_empty() {
            return Ok(());
        }
        use sqlx::Row;
        for row in rows {
            let id: Uuid = row.get("id");
            let scope = Scope {
                tenant: row.get("tenant_id"),
                user: row.get("user_id"),
            };
            let ciphertext = util::encrypt(
                &core.config.encryption_key,
                &row.get::<String, _>("content"),
                &util::owner(&scope, id),
            )?;
            sqlx::query("INSERT INTO agent_message_archive(message_id,ciphertext) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(id).bind(&ciphertext).execute(&mut *tx).await?;
            if row.get::<String, _>("role") == "user" {
                sqlx::query("INSERT INTO agent_intake(message_id,conversation_id,request_id,ciphertext) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(id).bind(row.get::<Uuid,_>("conversation_id")).bind(row.get::<Uuid,_>("request_id")).bind(ciphertext).execute(&mut *tx).await?;
                let digest = util::encrypt(
                    &core.config.encryption_key,
                    &fingerprint(&row.get::<String, _>("content")),
                    &util::owner(&scope, id),
                )?;
                sqlx::query("INSERT INTO agent_intake_fingerprints(message_id,ciphertext) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(id).bind(digest).execute(&mut *tx).await?;
            }
            sqlx::query("UPDATE agent_messages SET content='[历史内容已加密归档，等待净化]',status='queued',memory_status='pending',error=NULL WHERE id=$1").bind(id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
    }
}

pub async fn receive(
    core: Arc<Core>,
    scope: &Scope,
    conversation: Uuid,
    prompt: Prompt,
) -> ServiceResult<Thread> {
    util::text(&prompt.content, 16000, "消息")?;
    let content = &prompt.content;
    store::owned(&core.pool, scope, conversation).await?;
    let mut tx = core.pool.begin().await?;
    sqlx::query("SELECT id FROM agent_conversations WHERE id=$1 FOR UPDATE")
        .bind(conversation)
        .execute(&mut *tx)
        .await?;
    let duplicate: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM agent_messages WHERE conversation_id=$1 AND request_id=$2 AND role='user'",
    )
    .bind(conversation)
    .bind(prompt.request_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(message) = duplicate {
        let encrypted: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT ciphertext FROM agent_intake_fingerprints WHERE message_id=$1",
        )
        .bind(message)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(encrypted) = encrypted else {
            return Err(bad("此历史请求无法校验，请使用新的请求 ID"));
        };
        let prior = util::decrypt(
            &core.config.encryption_key,
            &encrypted,
            &util::owner(scope, message),
        )?;
        if !bool::from(prior.as_bytes().ct_eq(fingerprint(content).as_bytes())) {
            return Err(bad("同一请求 ID 不能提交不同内容"));
        }
        tx.rollback().await?;
        return receipt(&core, scope, conversation, prompt.request_id).await;
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM agent_messages WHERE conversation_id=$1")
            .bind(conversation)
            .fetch_one(&mut *tx)
            .await?;
    if count >= 398 {
        return Err(bad("会话已达 200 轮，请新建会话"));
    }
    let message = Uuid::new_v4();
    let ciphertext = util::encrypt(
        &core.config.encryption_key,
        content,
        &util::owner(scope, message),
    )?;
    for (id, role, body) in [
        (message, "user", "[已加密收件，等待整理]"),
        (Uuid::new_v4(), "assistant", "已收下，正在后台整理。"),
    ] {
        sqlx::query("INSERT INTO agent_messages(id,conversation_id,request_id,role,content,status,memory_status) VALUES($1,$2,$3,$4,$5,'queued','pending')")
            .bind(id).bind(conversation).bind(prompt.request_id).bind(role).bind(body).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO agent_intake(message_id,conversation_id,request_id,ciphertext) VALUES($1,$2,$3,$4)").bind(message).bind(conversation).bind(prompt.request_id).bind(ciphertext).execute(&mut *tx).await?;
    let digest = util::encrypt(
        &core.config.encryption_key,
        &fingerprint(content),
        &util::owner(scope, message),
    )?;
    sqlx::query("INSERT INTO agent_intake_fingerprints(message_id,ciphertext) VALUES($1,$2)")
        .bind(message)
        .bind(digest)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE agent_conversations SET updated_at=now() WHERE id=$1")
        .bind(conversation)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    receipt(&core, scope, conversation, prompt.request_id).await
}

async fn receipt(core: &Core, scope: &Scope, id: Uuid, request: Uuid) -> ServiceResult<Thread> {
    let conversation = store::owned(&core.pool, scope, id).await?;
    let rows=sqlx::query("SELECT id,role FROM agent_messages WHERE conversation_id=$1 AND request_id=$2 ORDER BY sequence")
        .bind(id).bind(request).fetch_all(&core.pool).await?;
    let messages = rows
        .into_iter()
        .map(|row| {
            let role: String = row.get("role");
            Message {
                id: row.get("id"),
                content: if role == "user" {
                    "[已加密收件，等待整理]"
                } else {
                    "已收下，正在后台整理。"
                }
                .into(),
                role,
                status: "queued".into(),
                error: None,
                tokens: None,
                source_id: None,
                memory_status: Some("pending".into()),
                citations: Vec::new(),
            }
        })
        .collect();
    Ok(Thread {
        conversation,
        messages,
    })
}
