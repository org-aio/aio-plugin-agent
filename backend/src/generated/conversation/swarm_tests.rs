use super::{model::*, service::AgentService, service_impl::AgentServiceImpl, swarm_tools};
use crate::configuration::{Gateway, RuntimeConfig};
use anyhow::{Context, Result};
use axum::{Json, Router, extract::State, routing::post};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn result<T>(value: ServiceResult<T>) -> Result<T> {
    value.map_err(|e| anyhow::anyhow!(e.1))
}
#[derive(Clone)]
struct Broker {
    devices: Value,
    tasks: Arc<Mutex<HashMap<String, Value>>>,
    concurrent: Arc<AtomicUsize>,
    maximum: Arc<AtomicUsize>,
    submitted: Arc<AtomicUsize>,
}
async fn invoke(State(broker): State<Broker>, Json(body): Json<Value>) -> Json<Value> {
    assert_eq!(body["tenantId"], "swarm-tenant");
    assert_eq!(body["userId"], "owner");
    assert!(matches!(
        body["capability"].as_str(),
        Some("workspace.execute" | "*")
    ));
    let id = body["requestId"].as_str().unwrap_or_default().to_owned();
    Json(match body["operation"].as_str().unwrap_or_default() {
        "list" => broker.devices.clone(),
        "submit" => {
            let running = broker.concurrent.fetch_add(1, Ordering::SeqCst) + 1;
            broker.maximum.fetch_max(running, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(40)).await;
            let mut tasks = broker.tasks.lock().await;
            let value=tasks.entry(id.clone()).or_insert_with(||{
                broker.submitted.fetch_add(1,Ordering::SeqCst);
                json!({"id":id,"worker_id":body["workerId"],"state":"queued","result":null,"error":null})
            }).clone();
            broker.concurrent.fetch_sub(1, Ordering::SeqCst);
            value
        }
        "task" => broker.tasks.lock().await[body["taskId"].as_str().unwrap()].clone(),
        "cancel" => {
            let mut tasks = broker.tasks.lock().await;
            let task = tasks.get_mut(body["taskId"].as_str().unwrap()).unwrap();
            if matches!(task["state"].as_str(), Some("queued" | "running")) {
                task["state"] = json!("cancelled");
            }
            task.clone()
        }
        _ => panic!("未知操作"),
    })
}

#[tokio::test]
#[ignore = "需要隔离 PostgreSQL：AIO_INPUT_TEST_CONFIG"]
async fn swarm_dispatch_is_scoped_concurrent_durable_and_cancellable() -> Result<()> {
    let config: Value = serde_json::from_str(&std::fs::read_to_string(std::env::var(
        "AIO_INPUT_TEST_CONFIG",
    )?)?)?;
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let broker = Broker {
        devices: json!([{"id":a,"label":"Mac mini","status":"online","platform":"darwin"},{"id":b,"label":"MacBook","status":"online","platform":"darwin"}]),
        tasks: Default::default(),
        concurrent: Default::default(),
        maximum: Default::default(),
        submitted: Default::default(),
    };
    let socket = std::env::temp_dir().join(format!("swarm-{}.sock", Uuid::new_v4().simple()));
    let listener = tokio::net::UnixListener::bind(&socket)?;
    let router = Router::new()
        .route("/workers", post(invoke))
        .with_state(broker.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    let settings = RuntimeConfig {
        database_url: config["databaseUrl"]
            .as_str()
            .context("databaseUrl")?
            .into(),
        encryption_key: [42; 32],
        gateway: Some(Gateway {
            socket: socket.clone(),
            token: "fixture-token".into(),
            worker_capabilities: vec!["workspace.execute".into()],
        }),
        allowed_endpoints: Default::default(),
        generation_timeout: Duration::from_secs(30),
        allow_loopback: false,
        memory: None,
    };
    let scope = Scope {
        tenant: "swarm-tenant".into(),
        user: "owner".into(),
        context_id: None,
    };
    let service = AgentServiceImpl::connect(settings.clone()).await?;
    let conversation = result(
        service
            .create(
                &scope,
                ConversationDraft {
                    title: "蜂群验收".into(),
                    provider_id: None,
                    space_id: None,
                },
            )
            .await,
    )?
    .id;
    let assistant = Uuid::new_v4();
    sqlx::query("INSERT INTO agent_messages(id,conversation_id,request_id,role,content,status,memory_status) VALUES($1,$2,$1,'assistant','已派发两个独立设备任务','generating','complete')").bind(assistant).bind(conversation).execute(&service.core.pool).await?;
    service
        .core
        .jobs
        .lock()
        .await
        .insert(conversation, CancellationToken::new());
    let tools = swarm_tools::tools(
        service.core.clone(),
        scope.clone(),
        assistant,
        None,
        "在 Mac mini 和 MacBook 检查项目",
    );
    let ambiguous = swarm_tools::tools(
        service.core.clone(),
        scope.clone(),
        assistant,
        None,
        "检查项目",
    );
    assert!(
        ambiguous[0]
            .invoke(json!({"groups":[{"label":"待选设备","input":{"action":"describe"}}]}))
            .await
            .unwrap_err()
            .is::<az_agent_engine::InputRequired>()
    );
    assert_eq!(broker.submitted.load(Ordering::SeqCst), 0);
    let args = json!({"groups":[{"device":"Mac mini","label":"mini 检查","input":{"action":"describe"}},{"device":"MacBook","label":"book 检查","input":{"action":"describe"}}]});
    let dispatched = tools[0].invoke(args.clone()).await?;
    assert_eq!(broker.submitted.load(Ordering::SeqCst), 2);
    assert_eq!(broker.maximum.load(Ordering::SeqCst), 2);
    tools[0].invoke(args.clone()).await?;
    assert_eq!(broker.submitted.load(Ordering::SeqCst), 2);
    let ids: Vec<Uuid> = dispatched["tasks"]
        .as_array()
        .context("tasks")?
        .iter()
        .map(|t| Uuid::parse_str(t["id"].as_str().unwrap()))
        .collect::<std::result::Result<_, _>>()?;
    let foreign = Scope {
        user: "other".into(),
        ..scope.clone()
    };
    let tenant = Scope {
        tenant: "other".into(),
        ..scope.clone()
    };
    assert!(service.swarm_tasks(&foreign, conversation).await.is_err());
    assert!(service.swarm_tasks(&tenant, conversation).await.is_err());
    assert!(
        service
            .cancel_swarm_task(&foreign, conversation, ids[0])
            .await
            .is_err()
    );
    assert!(
        tools[2]
            .invoke(json!({"task_ids":[Uuid::new_v4()]}))
            .await
            .is_err()
    );
    {
        let mut tasks = broker.tasks.lock().await;
        let task = tasks.get_mut(&ids[0].to_string()).context("task")?;
        task["state"] = json!("failed");
        task["result"] = json!({"success":false,"jobs":[{"id":"test","state":"failed","result":{"exitCode":7,"stdout":"日志".repeat(16000)}}]});
    }
    result(
        service
            .cancel_swarm_task(&scope, conversation, ids[1])
            .await,
    )?;
    tools[0].invoke(args).await?;
    assert_eq!(broker.submitted.load(Ordering::SeqCst), 2);
    let waited = tools[1].invoke(json!({"task_ids":ids})).await?;
    assert!(serde_json::to_vec(&waited)?.len() < 64000);
    let tasks = waited["tasks"].as_array().context("tasks")?;
    assert!(tasks.iter().any(|t| t["state"] == "cancelled"));
    let finished = tasks
        .iter()
        .find(|t| t["id"] == ids[0].to_string())
        .context("finished")?;
    assert_eq!(finished["result"]["success"], false);
    assert_eq!(finished["result"]["jobs"][0]["result"]["exitCode"], 7);
    let cipher: Vec<u8> =
        sqlx::query_scalar("SELECT ciphertext FROM agent_swarm_tasks WHERE id=$1")
            .bind(ids[0])
            .fetch_one(&service.core.pool)
            .await?;
    assert!(!String::from_utf8_lossy(&cipher).contains("日志"));
    service.core.jobs.lock().await.remove(&conversation);
    drop(tools);
    drop(ambiguous);
    service.shutdown().await;
    drop(service);
    let service = AgentServiceImpl::connect(settings).await?;
    assert_eq!(
        result(service.swarm_tasks(&scope, conversation).await)?.len(),
        2
    );
    if std::env::var_os("AIO_SWARM_BROWSER").is_some() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let app = crate::transport::router(
            Arc::new(AgentServiceImpl {
                core: service.core.clone(),
            }),
            crate::transport::Ingress {
                token: config["ingressToken"]
                    .as_str()
                    .context("ingressToken")?
                    .into(),
                tenant: None,
            },
        );
        let http = tokio::spawn(async move { axum::serve(listener, app).await });
        let status = tokio::process::Command::new("node")
            .arg("scripts/test-swarm-browser.mjs")
            .current_dir(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .context("root")?,
            )
            .env("AIO_PLUGIN_PORT", port.to_string())
            .env(
                "AIO_AGENT_DEV_CONFIG",
                std::env::var("AIO_INPUT_TEST_CONFIG")?,
            )
            .env("AIO_AGENT_PREVIEW_TENANT", "swarm-tenant")
            .env("AIO_AGENT_PREVIEW_USER", "owner")
            .status()
            .await?;
        http.abort();
        assert!(status.success(), "浏览器验收失败");
    }
    result(service.delete(&scope, conversation).await)?;
    service.shutdown().await;
    drop(service);
    server.abort();
    std::fs::remove_file(socket)?;
    Ok(())
}
