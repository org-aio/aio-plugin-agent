use super::{generation, memory, model::*, model_access, service_impl::Core, util};
use crate::runtime;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::Row;
use std::{
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::sync::mpsc;
use uuid::Uuid;

pub fn start(weak: Weak<Core>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tokio::join!(run(weak.clone(), false), run(weak, true));
    })
}

async fn run(weak: Weak<Core>, background: bool) {
    loop {
        let Some(core) = weak.upgrade() else {
            return;
        };
        tokio::select! {
            _=core.shutdown.cancelled()=>return,
            _=tokio::time::sleep(Duration::from_millis(500))=>{}
        }
        if core.config.memory.is_none() {
            continue;
        }
        let stop = core.shutdown.clone();
        tokio::select! {
            _=stop.cancelled()=>return,
            _=async {
                let result = if background {compile_one(&core).await} else {deliver(&core).await};
                if result.is_err() {
                    eprintln!("记忆任务暂不可用，已保留资料等待重试");
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            }=>{}
        }
    }
}

async fn deliver(core: &Arc<Core>) -> Result<()> {
    let rows = sqlx::query("SELECT i.message_id,i.conversation_id,i.request_id,i.ciphertext,i.state,c.tenant_id,c.user_id,c.space_id,c.provider_id,m.content,m.source_id,m.memory_status FROM agent_intake i JOIN agent_conversations c ON c.id=i.conversation_id JOIN agent_messages m ON m.id=i.message_id WHERE i.state IN ('pending','captured') AND i.available_at<=now() ORDER BY m.sequence LIMIT 8").fetch_all(&core.pool).await?;
    for row in rows {
        let message: Uuid = row.get("message_id");
        if deliver_message(core, row).await.is_err() {
            sqlx::query("UPDATE agent_intake SET attempts=attempts+1,available_at=now()+interval '15 seconds' WHERE message_id=$1")
                .bind(message).execute(&core.pool).await?;
        }
    }
    synchronize(core).await
}

async fn deliver_message(core: &Arc<Core>, row: sqlx::postgres::PgRow) -> Result<()> {
    let message: Uuid = row.get("message_id");
    let conversation: Uuid = row.get("conversation_id");
    let request: Uuid = row.get("request_id");
    let scope = Scope {
        tenant: row.get("tenant_id"),
        user: row.get("user_id"),
    };
    let mut space: Option<String> = row.get("space_id");
    let mut safe: String = row.get("content");
    let mut source: Option<String> = row.get("source_id");
    if row.get::<String, _>("state") == "pending" {
        let raw = util::decrypt(
            &core.config.encryption_key,
            &row.get::<Vec<u8>, _>("ciphertext"),
            &util::owner(&scope, message),
        )
        .context("加密收件不可读")?;
        let clarifies: Option<String> = sqlx::query_scalar("SELECT source_id FROM agent_messages WHERE conversation_id=$1 AND role='user' AND memory_status='quarantined' AND sequence<(SELECT sequence FROM agent_messages WHERE id=$2) ORDER BY sequence DESC LIMIT 1")
                .bind(conversation).bind(message).fetch_optional(&core.pool).await?.flatten();
        let captured = memory::invoke(core,&scope,"POST","/capture",json!({"requestId":request,"text":raw,"spaceId":space,"origin":"chat","reference":conversation,"clarifies":clarifies}),false).await;
        let captured: memory::CapturedSource = serde_json::from_value(captured?)?;
        safe = captured.text;
        source = Some(captured.id.clone());
        space = Some(captured.space_id.clone());
        let mut tx = core.pool.begin().await?;
        sqlx::query("UPDATE agent_conversations SET space_id=$2 WHERE id=$1 AND (space_id IS NULL OR space_id=$2)").bind(conversation).bind(&captured.space_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE agent_messages SET source_id=$3,memory_status=$4,content=CASE WHEN role='user' THEN $5 ELSE content END,status=CASE WHEN role='user' THEN 'complete' ELSE status END WHERE conversation_id=$1 AND request_id=$2")
                .bind(conversation).bind(request).bind(&captured.id).bind(&captured.status).bind(&safe).execute(&mut *tx).await?;
        sqlx::query("UPDATE agent_intake SET state='captured',ciphertext=NULL WHERE message_id=$1")
            .bind(message)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        if let Some(id) = clarifies
            && let Ok(updated) = memory::invoke(
                core,
                &scope,
                "GET",
                &format!("/sources/{id}"),
                Value::Null,
                false,
            )
            .await
        {
            sqlx::query("UPDATE agent_messages SET content=CASE WHEN role='user' THEN $2 ELSE content END,memory_status=$3 WHERE conversation_id=$4 AND source_id=$1")
                        .bind(id).bind(updated["text"].as_str().unwrap_or("[资料已保密暂存]"))
                        .bind(updated["status"].as_str().unwrap_or("quarantined")).bind(conversation).execute(&core.pool).await?;
        }
    }
    if core.jobs.lock().await.contains_key(&conversation) {
        return Ok(());
    }
    let route = memory::route(
        core,
        &scope,
        space.as_deref().context("记忆空间尚未绑定")?,
        source.as_deref().context("来源尚未保存")?,
    )
    .await?;
    sqlx::query("UPDATE agent_messages SET route=$3,matched_node_ids=$4,activated_node_ids=$5 WHERE conversation_id=$1 AND request_id=$2")
        .bind(conversation).bind(request).bind(&route.route)
        .bind(serde_json::to_value(&route.matched_node_ids)?)
        .bind(serde_json::to_value(&route.activated_node_ids)?)
        .execute(&core.pool).await?;
    let connection = if route.route != "model" {
        None
    } else {
        model_access::conversation(
            core,
            &scope,
            row.get("provider_id"),
            space.as_deref().context("记忆空间尚未绑定")?,
        )
        .await?
    };
    if let Some(connection) = connection {
        if let Err(error) = generation::respond(
            core.clone(),
            &scope,
            conversation,
            Prompt {
                request_id: request,
                content: safe,
            },
            (route.context, route.citations),
            connection,
        )
        .await
        {
            if error.0.is_client_error() && error.0 != axum::http::StatusCode::CONFLICT {
                sqlx::query("UPDATE agent_messages SET status='failed',error=$3 WHERE conversation_id=$1 AND request_id=$2 AND role='assistant'")
                        .bind(conversation).bind(request).bind(error.1).execute(&core.pool).await?;
            } else {
                return Ok(());
            }
        }
    } else {
        sqlx::query("UPDATE agent_messages SET status='complete',content=$3,citations=$4,tokens=0 WHERE conversation_id=$1 AND request_id=$2 AND role='assistant'")
                .bind(conversation).bind(request)
                .bind(route.reply.as_deref().unwrap_or("已收下，资料已保存。模型配置完成后继续整理。"))
                .bind(serde_json::to_value(&route.citations)?).execute(&core.pool).await?;
    }
    sqlx::query("UPDATE agent_intake SET state='complete' WHERE message_id=$1")
        .bind(message)
        .execute(&core.pool)
        .await?;
    Ok(())
}

async fn synchronize(core: &Core) -> Result<()> {
    let rows=sqlx::query("SELECT * FROM (SELECT DISTINCT c.tenant_id,c.user_id,m.source_id FROM agent_messages m JOIN agent_conversations c ON c.id=m.conversation_id WHERE m.source_id IS NOT NULL AND m.memory_status IN ('pending','processing')) pending ORDER BY random() LIMIT 16").fetch_all(&core.pool).await?;
    for row in rows {
        let scope = Scope {
            tenant: row.get("tenant_id"),
            user: row.get("user_id"),
        };
        let id: String = row.get("source_id");
        if let Ok(source) = memory::invoke(
            core,
            &scope,
            "GET",
            &format!("/sources/{id}"),
            Value::Null,
            false,
        )
        .await
        {
            update_status(core, &scope, &id, &source).await?;
        }
    }
    Ok(())
}

async fn update_status(core: &Core, scope: &Scope, id: &str, source: &Value) -> Result<()> {
    sqlx::query("UPDATE agent_messages SET memory_status=$2 WHERE source_id=$1 AND conversation_id IN (SELECT id FROM agent_conversations WHERE tenant_id=$3)")
        .bind(id).bind(source["status"].as_str().unwrap_or("pending")).bind(&scope.tenant).execute(&core.pool).await?;
    Ok(())
}

async fn compile_one(core: &Arc<Core>) -> Result<()> {
    // 前台生成优先；每次只领取一个后台任务，避免耗尽模型并发。
    if !core.jobs.lock().await.is_empty() {
        return Ok(());
    }
    let spaces = sqlx::query("SELECT * FROM (SELECT tenant_id,user_id,space_id FROM agent_conversations WHERE space_id IS NOT NULL UNION SELECT p.tenant_id,p.user_id,g.space_id FROM agent_model_grants g JOIN agent_providers p ON p.id=g.provider_id) scopes ORDER BY random() LIMIT 200").fetch_all(&core.pool).await?;
    for row in spaces {
        let scope = Scope {
            tenant: row.get("tenant_id"),
            user: row.get("user_id"),
        };
        let space: String = row.get("space_id");
        let Ok(task) = memory::invoke(
            core,
            &scope,
            "POST",
            "/tasks/claim",
            json!({"spaceId":space}),
            false,
        )
        .await
        else {
            continue;
        };
        if task.is_null() {
            continue;
        }
        let id = task["id"].as_str().context("整理任务无效")?;
        let lease = task["lease"].as_str().context("整理租约无效")?;
        let foreground = async {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if !core.jobs.lock().await.is_empty() {
                    break;
                }
            }
        };
        let result = tokio::select! {
            result=compile(core,&scope,&task)=>Some(result),
            _=foreground=>None,
        };
        let failure_code = if result.is_none() {
            "cancelled"
        } else {
            "invalid_result"
        };
        let submitted = match result {
            Some(Ok(result)) => {
                memory::invoke(
                    core,
                    &scope,
                    "POST",
                    &format!("/tasks/{id}/submit"),
                    json!({"lease":lease,"result":result}),
                    false,
                )
                .await
            }
            _ => Err(anyhow::anyhow!("模型整理失败")),
        };
        if submitted.is_err() {
            memory::invoke(
                core,
                &scope,
                "POST",
                &format!("/tasks/{id}/fail"),
                json!({"lease":lease,"code":failure_code}),
                false,
            )
            .await?;
        }
        let source = memory::invoke(
            core,
            &scope,
            "GET",
            &format!("/sources/{id}"),
            Value::Null,
            false,
        )
        .await?;
        update_status(core, &scope, id, &source).await?;
        break;
    }
    Ok(())
}

async fn compile(core: &Arc<Core>, scope: &Scope, task: &Value) -> Result<Value> {
    let _permit = core
        .quota
        .clone()
        .try_acquire_owned()
        .context("模型并发已满")?;
    let provider_id = Uuid::parse_str(task["modelBinding"].as_str().context("整理模型未绑定")?)?;
    let space = task["source"]["spaceId"].as_str().context("整理空间无效")?;
    let connection = model_access::shared(core, scope, provider_id, space).await?;
    let messages = vec![
        json!({"role":"system","content":task["instructions"]}),
        json!({"role":"user","content":serde_json::to_string(&json!({"source":task["source"],"existing":task["existing"]}))?}),
    ];
    let (sender, mut receiver) = mpsc::channel(32);
    let upstream = runtime::generate(
        &core.config.engine,
        &core.client,
        &connection.endpoint,
        &connection.model,
        connection.secret.as_deref(),
        messages,
        None,
        sender,
    );
    let collect = async {
        let mut output = String::new();
        while let Some(delta) = receiver.recv().await {
            if let runtime::Delta::Text(text) = delta {
                output.push_str(&text);
                ensure!(output.len() <= 100_000, "整理结果过大");
            }
        }
        Ok::<_, anyhow::Error>(output)
    };
    let (_, output) = tokio::time::timeout(core.config.generation_timeout, async {
        tokio::try_join!(upstream, collect)
    })
    .await
    .context("整理超时")??;
    serde_json::from_str(&output).context("整理结果不是有效 JSON")
}
