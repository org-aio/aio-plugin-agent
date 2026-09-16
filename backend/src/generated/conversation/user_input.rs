use super::{model::*, service_impl::Core, store, util};
use az_agent_engine::{InputRequired, RunState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

/// 加密保存执行位置；模型凭据在恢复时重新取得，不进入检查点。
#[derive(Serialize, Deserialize)]
pub(super) struct Checkpoint {
    pub state: RunState,
    pub input: InputRequired,
    pub prompt: String,
}

pub(super) async fn pending(
    core: &Core,
    scope: &Scope,
    conversation: Uuid,
) -> ServiceResult<Option<UserInputRequest>> {
    let row = sqlx::query("SELECT id,assistant_id,ciphertext FROM agent_user_inputs WHERE conversation_id=$1 AND state='pending'")
        .bind(conversation).fetch_optional(&core.pool).await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let id: Uuid = row.get("id");
    let checkpoint = decode(core, scope, id, row.get("ciphertext"))?;
    Ok(Some(UserInputRequest {
        id,
        assistant_id: row.get("assistant_id"),
        questions: questions(&checkpoint)?,
    }))
}

fn decode(core: &Core, scope: &Scope, id: Uuid, cipher: Vec<u8>) -> ServiceResult<Checkpoint> {
    let plain = util::decrypt(
        &core.config.encryption_key,
        &cipher,
        &util::owner(scope, id),
    )?;
    serde_json::from_str(&plain)
        .map_err(anyhow::Error::from)
        .map_err(Into::into)
}
fn questions(checkpoint: &Checkpoint) -> ServiceResult<Vec<InputQuestion>> {
    serde_json::from_value(checkpoint.input.request["questions"].clone())
        .map_err(anyhow::Error::from)
        .map_err(Into::into)
}

pub(super) async fn save(
    core: &Core,
    scope: &Scope,
    conversation: Uuid,
    assistant: Uuid,
    checkpoint: Checkpoint,
    content: &str,
    tokens: Option<i64>,
) -> anyhow::Result<()> {
    let questions: Vec<InputQuestion> =
        serde_json::from_value(checkpoint.input.request["questions"].clone())?;
    super::input_tool::validate(&questions)?;
    let id = Uuid::new_v4();
    let cipher = util::encrypt(
        &core.config.encryption_key,
        &serde_json::to_string(&checkpoint)?,
        &util::owner(scope, id),
    )?;
    let mut tx = core.pool.begin().await?;
    sqlx::query("SELECT id FROM agent_conversations WHERE id=$1 FOR UPDATE")
        .bind(conversation)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO agent_user_inputs(id,conversation_id,assistant_id,ciphertext,state) VALUES($1,$2,$3,$4,'pending')")
        .bind(id).bind(conversation).bind(assistant).bind(cipher).execute(&mut *tx).await?;
    sqlx::query("UPDATE agent_messages SET status='awaiting_input',content=$2,tokens=$3,error=NULL WHERE id=$1")
        .bind(assistant).bind(content).bind(tokens).execute(&mut *tx).await?;
    sqlx::query("UPDATE agent_conversations SET updated_at=now() WHERE id=$1")
        .bind(conversation)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub(super) async fn answer(
    core: Arc<Core>,
    scope: &Scope,
    conversation: Uuid,
    answer: InputAnswer,
) -> ServiceResult<Thread> {
    let thread = super::memory::safe_thread(&core, scope, conversation, true).await?;
    let mut jobs = core.jobs.lock().await;
    let row = sqlx::query("SELECT assistant_id,ciphertext,answer_ciphertext,state FROM agent_user_inputs WHERE id=$1 AND conversation_id=$2")
        .bind(answer.request_id).bind(conversation).fetch_optional(&core.pool).await?.ok_or_else(missing)?;
    let encoded = serde_json::to_string(&answer.answers).map_err(anyhow::Error::from)?;
    if row.get::<String, _>("state") == "answered" {
        let prior = util::decrypt(
            &core.config.encryption_key,
            &row.get::<Vec<u8>, _>("answer_ciphertext"),
            &util::owner(scope, answer.request_id),
        )?;
        if prior != encoded {
            return Err(conflict("该问题已经提交不同答案"));
        }
        return Ok(thread);
    }
    if row.get::<String, _>("state") != "pending" || jobs.contains_key(&conversation) {
        return Err(conflict("该问题已结束或会话仍在执行，请刷新"));
    }
    let assistant: Uuid = row.get("assistant_id");
    let message = thread
        .messages
        .iter()
        .find(|message| message.id == assistant)
        .ok_or_else(missing)?;
    if message.status != "awaiting_input" || message.memory_status.as_deref() == Some("unavailable")
    {
        return Err(conflict("原任务不可继续，请取消后重新发送"));
    }
    let mut checkpoint = decode(&core, scope, answer.request_id, row.get("ciphertext"))?;
    super::input_tool::validate_answers(&questions(&checkpoint)?, &answer)
        .map_err(|e| bad(&e.to_string()))?;
    let mut selected = thread.conversation.worker_id;
    if checkpoint.input.request["kind"] == "device" {
        let id = Uuid::parse_str(&answer.answers["device"]).map_err(|_| bad("设备选择无效"))?;
        let tools = super::device_tools::tools(&core, scope, assistant, None, "");
        let tool = tools.first().ok_or_else(|| bad("设备能力已关闭"))?;
        let devices = tool.invoke(json!({})).await?;
        super::device_routing::select(&devices, Some(id), "")
            .map_err(|_| conflict("选定设备已离线或撤权，请恢复设备后重试，或取消任务"))?;
        selected = Some(id);
    }
    if !checkpoint.input.retry {
        checkpoint.state.answer(json!({"answers":answer.answers}))?;
    }
    let model = super::model_access::conversation(
        &core,
        scope,
        thread.conversation.provider_id,
        thread.conversation.model.as_deref(),
        thread.conversation.space_id.as_deref().unwrap_or(""),
    )
    .await?
    .ok_or_else(|| bad("请先配置模型"))?;
    let permit = core
        .quota
        .clone()
        .try_acquire_owned()
        .map_err(|_| conflict("生成并发已满，请稍后重试"))?;
    let tools = super::generation::tools(
        &core,
        scope,
        assistant,
        selected,
        &checkpoint.prompt,
        thread.conversation.space_id,
        message.source_id.clone(),
    )
    .await?;
    let command =
        super::device_command::prepare(&core, scope, assistant, selected, &checkpoint.prompt);
    let mut tx = core.pool.begin().await?;
    sqlx::query("SELECT id FROM agent_conversations WHERE id=$1 FOR UPDATE")
        .bind(conversation)
        .execute(&mut *tx)
        .await?;
    let pending: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_user_inputs WHERE id=$1 AND state='pending')",
    )
    .bind(answer.request_id)
    .fetch_one(&mut *tx)
    .await?;
    if !pending {
        return Err(conflict("该问题已结束，请刷新"));
    }
    sqlx::query("UPDATE agent_conversations SET worker_id=$2 WHERE id=$1")
        .bind(conversation)
        .bind(selected)
        .execute(&mut *tx)
        .await?;
    let cipher = util::encrypt(
        &core.config.encryption_key,
        &encoded,
        &util::owner(scope, answer.request_id),
    )?;
    sqlx::query("UPDATE agent_user_inputs SET state='answered',answer_ciphertext=$2 WHERE id=$1")
        .bind(answer.request_id)
        .bind(cipher)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE agent_messages SET status='generating' WHERE id=$1")
        .bind(assistant)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let cancel = core.shutdown.child_token();
    jobs.insert(conversation, cancel.clone());
    let task_core = core.clone();
    let task_scope = scope.clone();
    let initial_content = message.content.clone();
    tokio::spawn(async move {
        super::generation::run(
            task_core.clone(),
            task_scope,
            conversation,
            assistant,
            model.endpoint,
            model.model,
            model.secret,
            checkpoint.state.messages.clone(),
            tools,
            command,
            cancel,
            permit,
            Some(checkpoint.state),
            checkpoint.prompt,
            initial_content,
        )
        .await;
        task_core.jobs.lock().await.remove(&conversation);
    });
    drop(jobs);
    super::memory::safe_thread(&core, scope, conversation, false).await
}

pub(super) async fn devices(core: &Core, scope: &Scope) -> ServiceResult<Value> {
    let tools = super::device_tools::tools(core, scope, Uuid::nil(), None, "");
    let Some(tool) = tools.first() else {
        return Ok(json!([]));
    };
    tool.invoke(json!({})).await.map_err(Into::into)
}

pub(super) async fn select_device(
    core: &Core,
    scope: &Scope,
    conversation: Uuid,
    selection: DeviceSelection,
) -> ServiceResult<Conversation> {
    store::owned(&core.pool, scope, conversation).await?;
    let jobs = core.jobs.lock().await;
    if jobs.contains_key(&conversation) {
        return Err(conflict("请先停止当前任务"));
    }
    if pending(core, scope, conversation).await?.is_some() {
        return Err(conflict("请回答或取消当前问题"));
    }
    if let Some(id) = selection.worker_id {
        let devices = devices(core, scope).await?;
        if !devices
            .as_array()
            .is_some_and(|items| items.iter().any(|device| device["id"] == id.to_string()))
        {
            return Err(missing());
        }
    }
    sqlx::query("UPDATE agent_conversations SET worker_id=$2 WHERE id=$1")
        .bind(conversation)
        .bind(selection.worker_id)
        .execute(&core.pool)
        .await?;
    store::owned(&core.pool, scope, conversation).await
}

pub(super) async fn cancel(core: &Core, conversation: Uuid) -> ServiceResult<()> {
    let mut tx = core.pool.begin().await?;
    sqlx::query("SELECT id FROM agent_conversations WHERE id=$1 FOR UPDATE")
        .bind(conversation)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE agent_messages SET status='cancelled' WHERE conversation_id=$1 AND status='awaiting_input'").bind(conversation).execute(&mut *tx).await?;
    sqlx::query("UPDATE agent_user_inputs SET state='cancelled' WHERE conversation_id=$1 AND state='pending'").bind(conversation).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
