use super::{device_tools, model::*, service_impl::Core, store, util};
use anyhow::{Context, Result, ensure};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub(super) async fn conversation(core: &Core, scope: &Scope, assistant: Uuid) -> Result<Uuid> {
    sqlx::query_scalar("SELECT c.id FROM agent_conversations c JOIN agent_messages m ON m.conversation_id=c.id WHERE m.id=$1 AND c.tenant_id=$2 AND c.user_id=$3 AND m.role='assistant'")
        .bind(assistant).bind(&scope.tenant).bind(&scope.user).fetch_optional(&core.pool).await?.context("会话任务不存在")
}

/// 派发时保存关联与展示名称；终态回执由读取流程加密缓存。
pub(super) async fn record(
    core: &Core,
    conversation: Uuid,
    task: &SwarmTask,
    capability: &str,
) -> Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM agent_swarm_tasks WHERE assistant_id=$1")
            .bind(task.assistant_id)
            .fetch_one(&core.pool)
            .await?;
    ensure!(count < 32, "单轮设备任务已达到上限");
    let cancelled: bool = sqlx::query_scalar("INSERT INTO agent_swarm_tasks(id,conversation_id,assistant_id,worker_id,device,label,capability) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO UPDATE SET id=EXCLUDED.id RETURNING cancelled")
        .bind(task.id).bind(conversation).bind(task.assistant_id).bind(task.worker_id).bind(&task.device).bind(&task.label).bind(capability).fetch_one(&core.pool).await?;
    Ok(!cancelled)
}

pub(super) async fn list(
    core: &Core,
    scope: &Scope,
    conversation: Uuid,
) -> ServiceResult<Vec<SwarmTask>> {
    let mut tasks = read(core, scope, conversation, None).await?;
    let mut retained = false;
    // 任务列表只携带最新截图，避免每次轮询反复传输数十张历史图片。
    for task in &mut tasks {
        let Some(content) = task
            .result
            .as_mut()
            .and_then(|value| value.get_mut("content"))
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for item in content {
            if item["type"] != "image" {
                continue;
            }
            if retained {
                *item = json!({"type":"text","text":"历史截图已省略，查看最新桌面任务的截图。"});
            }
            retained = true;
        }
    }
    Ok(tasks)
}

pub(super) async fn read(
    core: &Core,
    scope: &Scope,
    conversation: Uuid,
    ids: Option<&[Uuid]>,
) -> ServiceResult<Vec<SwarmTask>> {
    store::owned(&core.pool, scope, conversation).await?;
    let rows = sqlx::query("SELECT * FROM agent_swarm_tasks WHERE conversation_id=$1 AND ($2::uuid[] IS NULL OR id=ANY($2)) ORDER BY created_at DESC LIMIT 32")
        .bind(conversation).bind(ids).fetch_all(&core.pool).await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let broker = device_tools::broker(core, scope, Uuid::nil(), None, "");
    let tasks = stream::iter(rows.into_iter().map(|row| {
        let broker = broker.clone();
        async move {
            let mut task = SwarmTask {
                id: row.get("id"),
                assistant_id: row.get("assistant_id"),
                worker_id: row.get("worker_id"),
                device: row.get("device"),
                label: row.get("label"),
                state: "unconfirmed".into(),
                result: None,
                error: Some("暂未取得设备回执，请刷新核对；不会自动重派任务。".into()),
            };
            if terminal(&row.get::<String, _>("state"))
                && let Some(cipher) = row.get::<Option<Vec<u8>>, _>("ciphertext")
                    && let Ok(plain) = util::decrypt(
                        &core.config.encryption_key,
                        &cipher,
                        &util::owner(scope, task.id),
                    )
                        && let Ok(cached) = serde_json::from_str::<SwarmTask>(&plain) {
                            return cached;
                        }
            let Some(broker) = broker else {
                return task;
            };
            let response = broker
                .request(
                    json!({"operation":"task","capability":row.get::<String, _>("capability"),"taskId":task.id}),
                )
                .await;
            if let Ok(value) = response
                && value["id"] == task.id.to_string()
                    && value["worker_id"] == task.worker_id.to_string()
                {
                    task.state = value["state"].as_str().unwrap_or("unconfirmed").into();
                    task.result = value.get("result").filter(|r| !r.is_null()).cloned();
                    task.error = value["error"].as_str().map(str::to_owned);
                }
            if row.get::<bool, _>("cancelled")
                && matches!(task.state.as_str(), "queued" | "running" | "unconfirmed")
            {
                task.state = "cancelling".into();
                task.error = Some("已请求停止，等待设备确认；断网时受任务租约约束。".into());
            }
            if row.get::<String, _>("capability") == "workspace.execute" {
                task.result = task.result.map(compact_result);
            }
            if terminal(&task.state)
                && let Ok(plain) = serde_json::to_string(&task)
                    && let Ok(cipher) = util::encrypt(
                        &core.config.encryption_key,
                        &plain,
                        &util::owner(scope, task.id),
                    )
                        && sqlx::query(
                            "UPDATE agent_swarm_tasks SET state=$2,ciphertext=$3 WHERE id=$1",
                        )
                        .bind(task.id)
                        .bind(&task.state)
                        .bind(cipher)
                        .execute(&core.pool)
                        .await
                        .is_err()
                        {
                            task.error =
                                Some("结果已收到，持久化暂时失败；可刷新重新查询。".into());
                        }
            task
        }
    }))
    .buffered(4)
    .collect()
    .await;
    Ok(tasks)
}

pub(super) fn terminal(state: &str) -> bool {
    matches!(state, "complete" | "failed" | "cancelled" | "interrupted")
}

pub(super) async fn owned_ids(core: &Core, conversation: Uuid, ids: &[Uuid]) -> Result<()> {
    ensure!(
        !ids.is_empty() && ids.len() <= 32,
        "必须指定 1 至 32 个任务"
    );
    let found: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM agent_swarm_tasks WHERE conversation_id=$1 AND id=ANY($2)",
    )
    .bind(conversation)
    .bind(ids)
    .fetch_all(&core.pool)
    .await?;
    ensure!(
        ids.iter().all(|id| found.contains(id)),
        "任务不属于当前会话"
    );
    Ok(())
}

/// 调用方与派发持有同一 jobs 锁，取消后不会再提交新的设备任务。
pub(super) async fn cancel(
    core: &Core,
    scope: &Scope,
    conversation: Uuid,
    ids: Option<&[Uuid]>,
) -> Result<()> {
    if let Some(ids) = ids {
        owned_ids(core, conversation, ids).await?;
    }
    let tasks = sqlx::query("UPDATE agent_swarm_tasks SET cancelled=true WHERE conversation_id=$1 AND state NOT IN ('complete','failed','cancelled','interrupted') AND ($2::uuid[] IS NULL OR id=ANY($2)) RETURNING id,capability")
        .bind(conversation).bind(ids).fetch_all(&core.pool).await?;
    if tasks.is_empty() {
        return Ok(());
    }
    let broker =
        device_tools::broker(core, scope, Uuid::nil(), None, "").context("设备能力已关闭")?;
    let outcomes = stream::iter(tasks.into_iter().map(|row| {
        let id: Uuid = row.get("id");
        let capability: String = row.get("capability");
        let broker = broker.clone();
        async move {
            broker
                .request(json!({"operation":"cancel","capability":capability,"taskId":id}))
                .await
        }
    }))
    .buffer_unordered(4)
    .collect::<Vec<Result<Value>>>()
    .await;
    ensure!(
        outcomes.iter().all(Result::is_ok),
        "停止请求尚未全部确认，请重试停止操作"
    );
    Ok(())
}

// 保留状态、退出码和哈希，正文/日志超过预算时明确标记截断，避免撑爆模型上下文。
pub(super) fn compact_result(mut value: Value) -> Value {
    fn trim(value: &mut Value, limit: usize) {
        match value {
            Value::String(text) if text.len() > limit => {
                let end = text
                    .char_indices()
                    .map(|(i, _)| i)
                    .take_while(|i| *i <= limit)
                    .last()
                    .unwrap_or(0);
                text.truncate(end);
                text.push_str("…[已截断，完整输出在设备任务回执中]");
            }
            Value::Array(items) => {
                for item in items {
                    trim(item, limit);
                }
            }
            Value::Object(items) => {
                for item in items.values_mut() {
                    trim(item, limit);
                }
            }
            _ => {}
        }
    }
    if serde_json::to_vec(&value).is_ok_and(|b| b.len() > 12000) {
        trim(&mut value, 256);
    }
    value
}
