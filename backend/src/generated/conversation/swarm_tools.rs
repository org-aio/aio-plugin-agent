use super::{
    device_routing, device_tools, model::*, service_impl::Core, swarm_model::*, swarm_store,
};
use crate::runtime::Tool;
use anyhow::{Context, Result, ensure};
use az_agent_engine::InputRequired;
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

const CAPABILITY: &str = "workspace.execute";

pub(super) fn tools(
    core: Arc<Core>,
    scope: Scope,
    assistant: Uuid,
    selected: Option<Uuid>,
    prompt: &str,
) -> Vec<Arc<dyn Tool>> {
    if !core
        .config
        .gateway
        .as_ref()
        .is_some_and(|g| g.worker_capabilities.iter().any(|c| c == CAPABILITY))
    {
        return vec![];
    }
    let Some(broker) = device_tools::broker(&core, &scope, assistant, selected, prompt) else {
        return vec![];
    };
    let context = Arc::new(ContextData {
        core,
        scope,
        assistant,
        selected,
        prompt: prompt.into(),
        broker,
    });
    vec![
        Arc::new(DispatchTool(context.clone())),
        Arc::new(WaitTool(context.clone())),
        Arc::new(CancelTool(context)),
    ]
}
struct ContextData {
    core: Arc<Core>,
    scope: Scope,
    assistant: Uuid,
    selected: Option<Uuid>,
    prompt: String,
    broker: Arc<device_tools::Broker>,
}
struct DispatchTool(Arc<ContextData>);
struct WaitTool(Arc<ContextData>);
struct CancelTool(Arc<ContextData>);

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn target(devices: &Value, group: &Group, selected: Option<Uuid>, prompt: &str) -> Result<Value> {
    if let Some(name) = &group.device {
        let name = normalize(name);
        ensure!(!name.is_empty() && name.len() <= 160, "设备名称无效");
        let matches: Vec<_> = devices
            .as_array()
            .context("设备列表无效")?
            .iter()
            .filter(|d| normalize(d["label"].as_str().unwrap_or("")).contains(&name))
            .collect();
        ensure!(matches.len() == 1, "设备名称不唯一，请明确设备名称");
        let device = matches[0];
        ensure!(
            normalize(prompt).contains(&name)
                || selected.is_some_and(|id| device["id"] == id.to_string()),
            "设备目标必须来自用户当前要求或会话选择"
        );
        ensure!(device["status"] == "online", "指定设备离线，未转移任务");
        return Ok(device.clone());
    }
    device_routing::select(devices, selected, prompt).map_err(|error| {
        if let Some(input) = error.downcast_ref::<InputRequired>() {
            let mut input = input.clone();
            input.request["capability"] = json!(CAPABILITY);
            return input.into();
        }
        error
    })
}

fn request_id(assistant: Uuid, worker: Uuid, input: &Value) -> Result<Uuid> {
    let mut hash = Sha256::new();
    hash.update(assistant.as_bytes());
    hash.update(worker.as_bytes());
    hash.update(serde_json::to_vec(input)?);
    let digest = hash.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Ok(Uuid::from_bytes(bytes))
}

fn parameters_ids(maximum: usize) -> Value {
    json!({"type":"object","properties":{"task_ids":{"type":"array","items":{"type":"string","format":"uuid"},"minItems":1,"maxItems":maximum}},"required":["task_ids"],"additionalProperties":false})
}

#[async_trait::async_trait]
impl Tool for DispatchTool {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"swarm_dispatch","description":"派发独立任务给已配对设备。先用 input={action:describe} 查询本机授权工作区和命令；再用 action:run,jobs:[{id,workspace,operation,...}]。操作：git.status/git.diff/git.log；fs.read/fs.write(path,content,expectedHash)；project.run(command 为登记名称)。同目录任务串行，不同目录和设备可并发；有依赖的任务拆开轮次。device 只能引用用户明确点名的设备或会话目标，多设备不明确时自动提问。派发只代表入队，必须 swarm_wait 验收结果；项目运行的退出码和验收命令通过才算成功。不会自动修代码或提交Git。","parameters":{"type":"object","properties":{"groups":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","properties":{"device":{"type":"string"},"label":{"type":"string","maxLength":80},"input":{"type":"object","properties":{"action":{"type":"string","enum":["describe","run"]},"jobs":{"type":"array","maxItems":8,"items":{"type":"object","properties":{"id":{"type":"string","pattern":"^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$"},"workspace":{"type":"string"},"operation":{"type":"string","enum":["git.status","git.diff","git.log","fs.read","fs.write","project.run"]},"path":{"type":"string"},"content":{"type":"string"},"expectedHash":{"type":["string","null"]},"command":{"type":"string"}},"required":["id","workspace","operation"],"additionalProperties":false}}},"required":["action"],"additionalProperties":false}},"required":["label","input"],"additionalProperties":false}}},"required":["groups"],"additionalProperties":false}}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        let context = &self.0;
        let request: Dispatch = serde_json::from_value(arguments)?;
        ensure!(
            !request.groups.is_empty() && request.groups.len() <= 4,
            "每次派发 1 至 4 组任务"
        );
        let devices = context
            .broker
            .request(json!({"operation":"list","capability":CAPABILITY}))
            .await?;
        let mut prepared = Vec::new();
        // 全部设备先确定，再产生任何副作用，避免半途提问时重复派发前面的分支。
        for group in request.groups {
            ensure!(
                !group.label.trim().is_empty() && group.label.len() <= 240,
                "任务标题无效"
            );
            ensure!(
                serde_json::to_vec(&group.input)?.len() <= 32_768,
                "任务参数超过限额"
            );
            ensure!(
                matches!(group.input["action"].as_str(), Some("describe" | "run")),
                "未知工作区操作"
            );
            let device = target(&devices, &group, context.selected, &context.prompt)?;
            let worker = Uuid::parse_str(device["id"].as_str().context("设备 ID 缺失")?)?;
            let id = request_id(context.assistant, worker, &group.input)?;
            let task = SwarmTask {
                id,
                assistant_id: context.assistant,
                worker_id: worker,
                device: device["label"].as_str().unwrap_or("设备").into(),
                label: group.label,
                state: "queued".into(),
                result: None,
                error: None,
            };
            prepared.push((task, group.input));
        }
        let conversation =
            swarm_store::conversation(&context.core, &context.scope, context.assistant).await?;
        let jobs = context.core.jobs.lock().await;
        ensure!(
            jobs.get(&conversation)
                .is_some_and(|token| !token.is_cancelled()),
            "会话已经停止，未派发任务"
        );
        let mut dispatches = Vec::new();
        for (task, input) in prepared {
            let active = swarm_store::record(&context.core, conversation, &task).await?;
            dispatches.push((task, input, active));
        }
        let outcomes = stream::iter(dispatches.into_iter().map(|(mut task, input, active)| {
            let broker = context.broker.clone();
            async move {
                if !active { task.state = "cancelled".into(); return task; }
                match broker.request(json!({"operation":"submit","capability":CAPABILITY,"workerId":task.worker_id,"requestId":task.id,"input":input})).await {
                    Ok(value) if value["id"] == task.id.to_string() => {
                        task.state = value["state"].as_str().unwrap_or("unconfirmed").into();
                    }
                    _ => {
                        task.state = "unconfirmed".into();
                        task.error = Some("派发尚未确认，查询任务状态后重试同一请求；不要生成新任务 ID。".into());
                    }
                }
                task
            }
        })).buffered(4).collect::<Vec<_>>().await;
        drop(jobs);
        Ok(json!({"tasks":outcomes,"message":"任务已记录；入队不是成功，请等待实际执行回执。"}))
    }
}

#[async_trait::async_trait]
impl Tool for WaitTool {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"swarm_wait","description":"等待本会话指定任务，最多 20 秒。返回真实子任务状态、退出码、日志和结果；仍在运行可再次等待，不能编造验收。一个任务失败不阻断其他独立任务。","parameters":parameters_ids(4)}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        let args: TaskIds = serde_json::from_value(arguments)?;
        let context = &self.0;
        let conversation =
            swarm_store::conversation(&context.core, &context.scope, context.assistant).await?;
        swarm_store::owned_ids(&context.core, conversation, &args.task_ids).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let tasks = swarm_store::read(
                &context.core,
                &context.scope,
                conversation,
                Some(&args.task_ids),
            )
            .await
            .map_err(|e| anyhow::anyhow!(e.1))?;
            let tasks: Vec<_> = tasks
                .into_iter()
                .filter(|t| args.task_ids.contains(&t.id))
                .collect();
            if tasks.iter().all(|t| swarm_store::terminal(&t.state))
                || tokio::time::Instant::now() >= deadline
            {
                return Ok(json!({"tasks":tasks}));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
#[async_trait::async_trait]
impl Tool for CancelTool {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"swarm_cancel","description":"停止本会话指定设备任务。已结束的结果保留，停止不会回滚已写入的文件。","parameters":parameters_ids(32)}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        let args: TaskIds = serde_json::from_value(arguments)?;
        let context = &self.0;
        let conversation =
            swarm_store::conversation(&context.core, &context.scope, context.assistant).await?;
        let _jobs = context.core.jobs.lock().await;
        swarm_store::cancel(
            &context.core,
            &context.scope,
            conversation,
            Some(&args.task_ids),
        )
        .await?;
        Ok(json!({"state":"cancellation_requested","taskIds":args.task_ids}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn device_targets_come_from_user_and_ids_are_stable() -> Result<()> {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let assistant = Uuid::new_v4();
        let devices = json!([{"id":a,"label":"Mac mini","status":"online"},{"id":b,"label":"MacBook","status":"online"}]);
        let mut group = Group {
            device: None,
            label: "检查".into(),
            input: json!({"action":"describe"}),
        };
        let error = target(&devices, &group, None, "检查项目").unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<InputRequired>()
                .context("未请求选择")?
                .request["capability"],
            CAPABILITY
        );
        group.device = Some("Mac mini".into());
        assert!(target(&devices, &group, None, "检查项目").is_err());
        assert_eq!(
            target(&devices, &group, None, "在 Mac mini 和 MacBook 检查项目")?["id"],
            a.to_string()
        );
        assert_eq!(
            request_id(assistant, a, &group.input)?,
            request_id(assistant, a, &group.input)?
        );
        assert_ne!(
            request_id(assistant, a, &group.input)?,
            request_id(assistant, b, &group.input)?
        );
        Ok(())
    }
}
