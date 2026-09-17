use super::{generation, model::*, service::AgentService, service_impl::AgentServiceImpl};
use crate::configuration::{Gateway, RuntimeConfig};
use anyhow::Result;
use axum::{Json, Router, extract::State, routing::post};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

fn result<T>(value: ServiceResult<T>) -> Result<T> {
    value.map_err(|e| anyhow::anyhow!("{} {}", e.0, e.1))
}

#[derive(Clone)]
struct Broker {
    devices: Arc<std::sync::Mutex<Value>>,
    executed: Arc<AtomicUsize>,
    selected: Arc<std::sync::Mutex<Option<String>>>,
}
async fn workers(State(broker): State<Broker>, Json(body): Json<Value>) -> Json<Value> {
    assert_eq!(body["userId"], "owner");
    assert_eq!(body["tenantId"], "tenant");
    Json(match body["operation"].as_str().unwrap_or("") {
        "list" => broker.devices.lock().unwrap().clone(),
        "openApp" => {
            assert!(matches!(
                body["application"].as_str(),
                Some("QQ" | "WPS Office")
            ));
            broker.executed.fetch_add(1, Ordering::SeqCst);
            *broker.selected.lock().unwrap() = body["workerId"].as_str().map(str::to_owned);
            json!({"id":body["requestId"],"state":"complete","result":{"running":true,"pid":123}})
        }
        _ => panic!("未知请求"),
    })
}
async fn egress(Json(body): Json<Value>) -> impl axum::response::IntoResponse {
    if body.get("input").is_none() {
        return (
            [("content-type", "application/json")],
            json!({"data":[{"id":"fixture"}]}).to_string(),
        );
    }
    let input = body["input"].as_array().unwrap();
    let prompt = input
        .iter()
        .rev()
        .find(|item| item["role"] == "user")
        .unwrap();
    let opened = input
        .iter()
        .any(|item| item["type"] == "function_call_output" && item["call_id"] == "open_app");
    let answered = input
        .iter()
        .any(|item| item["type"] == "function_call_output" && item["call_id"] == "ask_config");
    let app = match prompt["content"].as_str() {
        Some("打开 QQ") => Some("QQ"),
        Some("打开wps输入helloworld") => Some("WPS Office"),
        _ => None,
    };
    let chunk = if let Some(app) = app.filter(|_| !opened) {
        assert!(
            body["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "device_open_application")
        );
        json!({"type":"function_call","call_id":"open_app","name":"device_open_application","arguments":json!({"application":app}).to_string()})
    } else if answered || (opened && app == Some("QQ")) {
        json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"已收到全部配置答案，继续原任务。"}]})
    } else if app == Some("WPS Office") {
        let arguments = json!({"questions":[{"id":"cell","title":"在哪里输入 helloworld？","options":[],"allowText":true}]}).to_string();
        json!({"type":"function_call","call_id":"ask_config","name":"request_user_input","arguments":arguments})
    } else {
        let arguments=json!({"questions":[{"id":"path","title":"配置存放在哪？","options":[],"allowText":true},{"id":"mode","title":"同步哪些内容？","options":[{"value":"shared","label":"共享配置"},{"value":"all","label":"全部配置"}],"allowText":false}]}).to_string();
        json!({"type":"function_call","call_id":"ask_config","name":"request_user_input","arguments":arguments})
    };
    (
        [("content-type", "text/event-stream")],
        format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":[chunk],"usage":{"total_tokens":1}}})
        ),
    )
}

async fn wait(service: &AgentServiceImpl, scope: &Scope, id: Uuid, status: &str) -> Result<Thread> {
    for _ in 0..100 {
        let thread = result(service.thread(scope, id).await)?;
        if thread
            .messages
            .iter()
            .any(|m| m.role == "assistant" && m.status == status)
            && !service.core.jobs.lock().await.contains_key(&id)
        {
            return Ok(thread);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    anyhow::bail!("没有进入 {status}")
}
async fn launch(service: &AgentServiceImpl, scope: &Scope, id: Uuid, prompt: &str) -> Result<()> {
    let request = Uuid::new_v4();
    for (role, status, content) in [("user", "complete", prompt), ("assistant", "queued", "")] {
        sqlx::query("INSERT INTO agent_messages(id,conversation_id,request_id,role,content,status,memory_status) VALUES($1,$2,$3,$4,$5,$6,'complete')")
            .bind(Uuid::new_v4()).bind(id).bind(request).bind(role).bind(content).bind(status).execute(&service.core.pool).await?;
    }
    result(
        generation::respond(
            service.core.clone(),
            scope,
            id,
            Prompt {
                request_id: request,
                content: prompt.into(),
            },
            (String::new(), vec![]),
            ModelConnection {
                endpoint: "https://fixture.invalid/v1".into(),
                model: "fixture".into(),
                secret: None,
            },
        )
        .await,
    )?;
    Ok(())
}

#[tokio::test]
#[ignore = "需要隔离 PostgreSQL，使用 scripts/setup-dev.mjs 创建 AIO_INPUT_TEST_CONFIG"]
async fn durable_device_question_survives_restart_and_enforces_ownership() -> Result<()> {
    let config: Value = serde_json::from_str(&std::fs::read_to_string(std::env::var(
        "AIO_INPUT_TEST_CONFIG",
    )?)?)?;
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let broker = Broker {
        devices: Arc::new(std::sync::Mutex::new(
            json!([{"id":a,"label":"Mac mini","platform":"darwin","status":"online"},{"id":b,"label":"MacBook","platform":"darwin","status":"online"}]),
        )),
        executed: Default::default(),
        selected: Default::default(),
    };
    let path = std::env::temp_dir().join(format!("ai-{}.sock", Uuid::new_v4().simple()));
    let listener = tokio::net::UnixListener::bind(&path)?;
    let router = Router::new()
        .route("/workers", post(workers))
        .route("/egress/responses", post(egress))
        .route(
            "/egress/models",
            axum::routing::get(|| async { Json(json!({"data":[{"id":"fixture"}]})) }),
        )
        .with_state(broker.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    let settings = RuntimeConfig {
        database_url: config["databaseUrl"].as_str().unwrap().into(),
        encryption_key: [42; 32],
        gateway: Some(Gateway {
            socket: path.clone(),
            token: "fixture-token".into(),
            worker_capabilities: vec!["desktop.open-app".into()],
        }),
        allowed_endpoints: ["https://fixture.invalid/v1".into()].into(),
        generation_timeout: Duration::from_secs(5),
        allow_loopback: false,
        memory: None,
    };
    let scope = Scope {
        tenant: "tenant".into(),
        user: "owner".into(),
        context_id: None,
    };
    let service = AgentServiceImpl::connect(settings.clone()).await?;
    let provider = Uuid::new_v4();
    sqlx::query("INSERT INTO agent_providers(id,tenant_id,user_id,label,endpoint,model) VALUES($1,'tenant','owner','Fixture','https://fixture.invalid/v1','fixture')").bind(provider).execute(&service.core.pool).await?;
    let conversation = result(
        service
            .create(
                &scope,
                ConversationDraft {
                    provider_id: Some(provider),
                    title: "设备提问".into(),
                    space_id: None,
                },
            )
            .await,
    )?
    .id;
    launch(&service, &scope, conversation, "打开 QQ").await?;
    let waiting = wait(&service, &scope, conversation, "awaiting_input").await?;
    let question = waiting.pending_input.unwrap();
    assert_eq!(question.questions[0].options.len(), 2);
    assert_eq!(broker.executed.load(Ordering::SeqCst), 0);
    let encrypted: Vec<u8> =
        sqlx::query_scalar("SELECT ciphertext FROM agent_user_inputs WHERE id=$1")
            .bind(question.id)
            .fetch_one(&service.core.pool)
            .await?;
    assert!(!String::from_utf8_lossy(&encrypted).contains("Mac mini"));
    service.shutdown().await;
    drop(service);
    let service = AgentServiceImpl::connect(settings).await?;
    assert_eq!(
        result(service.thread(&scope, conversation).await)?
            .pending_input
            .unwrap()
            .id,
        question.id
    );
    let answer = InputAnswer {
        request_id: question.id,
        answers: BTreeMap::from([("device".into(), a.to_string())]),
    };
    let foreign = Scope {
        user: "other".into(),
        ..scope.clone()
    };
    assert!(
        service
            .answer_input(&foreign, conversation, answer.clone())
            .await
            .is_err()
    );
    let tenant = Scope {
        tenant: "other".into(),
        ..scope.clone()
    };
    assert!(
        service
            .answer_input(&tenant, conversation, answer.clone())
            .await
            .is_err()
    );
    let invalid = InputAnswer {
        answers: BTreeMap::from([("device".into(), Uuid::new_v4().to_string())]),
        ..answer.clone()
    };
    assert!(
        service
            .answer_input(&scope, conversation, invalid)
            .await
            .is_err()
    );
    broker.devices.lock().unwrap()[0]["status"] = json!("offline");
    assert!(
        service
            .answer_input(&scope, conversation, answer.clone())
            .await
            .is_err()
    );
    assert_eq!(broker.executed.load(Ordering::SeqCst), 0);
    broker.devices.lock().unwrap()[0]["status"] = json!("online");
    result(
        service
            .answer_input(&scope, conversation, answer.clone())
            .await,
    )?;
    let completed = wait(&service, &scope, conversation, "complete").await?;
    assert!(completed.pending_input.is_none());
    assert_eq!(completed.conversation.worker_id, Some(a));
    assert_eq!(*broker.selected.lock().unwrap(), Some(a.to_string()));
    assert_eq!(broker.executed.load(Ordering::SeqCst), 1);
    result(
        service
            .answer_input(&scope, conversation, answer.clone())
            .await,
    )?;
    assert_eq!(broker.executed.load(Ordering::SeqCst), 1);
    result(
        service
            .select_device(&scope, conversation, DeviceSelection { worker_id: None })
            .await,
    )?;
    launch(&service, &scope, conversation, "打开 QQ").await?;
    let question = wait(&service, &scope, conversation, "awaiting_input")
        .await?
        .pending_input
        .unwrap();
    result(service.cancel(&scope, conversation).await)?;
    assert!(
        service
            .answer_input(
                &scope,
                conversation,
                InputAnswer {
                    request_id: question.id,
                    ..answer
                }
            )
            .await
            .is_err()
    );
    if std::env::var_os("AIO_INPUT_BROWSER").is_some() {
        result(
            service
                .select_device(&scope, conversation, DeviceSelection { worker_id: None })
                .await,
        )?;
        launch(&service, &scope, conversation, "打开 QQ").await?;
        wait(&service, &scope, conversation, "awaiting_input").await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let app = crate::transport::router(
            Arc::new(AgentServiceImpl {
                core: service.core.clone(),
            }),
            crate::transport::Ingress {
                token: config["ingressToken"].as_str().unwrap().into(),
                tenant: None,
            },
        );
        let http = tokio::spawn(async move { axum::serve(listener, app).await });
        let status = tokio::process::Command::new("node")
            .arg("scripts/test-input-browser.mjs")
            .current_dir(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap(),
            )
            .env("AIO_PLUGIN_PORT", port.to_string())
            .env(
                "AIO_AGENT_DEV_CONFIG",
                std::env::var("AIO_INPUT_TEST_CONFIG")?,
            )
            .env("AIO_AGENT_PREVIEW_TENANT", "tenant")
            .env("AIO_AGENT_PREVIEW_USER", "owner")
            .status()
            .await?;
        http.abort();
        assert!(status.success(), "浏览器验收失败");
    }
    // 完整复合请求必须到达模型；打开应用后仍需继续输入步骤，不能提前回复完成。
    let compound = result(
        service
            .create(
                &scope,
                ConversationDraft {
                    provider_id: Some(provider),
                    title: "打开并输入".into(),
                    space_id: None,
                },
            )
            .await,
    )?
    .id;
    result(
        service
            .select_device(&scope, compound, DeviceSelection { worker_id: Some(a) })
            .await,
    )?;
    let before = broker.executed.load(Ordering::SeqCst);
    launch(&service, &scope, compound, "打开wps输入helloworld").await?;
    let waiting = wait(&service, &scope, compound, "awaiting_input").await?;
    let question = waiting.pending_input.unwrap();
    assert_eq!(question.questions[0].title, "在哪里输入 helloworld？");
    assert_eq!(broker.executed.load(Ordering::SeqCst), before + 1);
    result(
        service
            .answer_input(
                &scope,
                compound,
                InputAnswer {
                    request_id: question.id,
                    answers: BTreeMap::from([("cell".into(), "A1".into())]),
                },
            )
            .await,
    )?;
    wait(&service, &scope, compound, "complete").await?;
    assert_eq!(broker.executed.load(Ordering::SeqCst), before + 1);
    let multi = result(
        service
            .create(
                &scope,
                ConversationDraft {
                    provider_id: Some(provider),
                    title: "多问题验收".into(),
                    space_id: None,
                },
            )
            .await,
    )?
    .id;
    let request = Uuid::new_v4();
    for (role, status, content) in [
        ("user", "complete", "配置同步"),
        ("assistant", "queued", ""),
    ] {
        sqlx::query("INSERT INTO agent_messages(id,conversation_id,request_id,role,content,status,memory_status) VALUES($1,$2,$3,$4,$5,$6,'complete')").bind(Uuid::new_v4()).bind(multi).bind(request).bind(role).bind(content).bind(status).execute(&service.core.pool).await?;
    }
    result(
        generation::respond(
            service.core.clone(),
            &scope,
            multi,
            Prompt {
                request_id: request,
                content: "配置同步".into(),
            },
            (String::new(), vec![]),
            ModelConnection {
                endpoint: "https://fixture.invalid/v1".into(),
                model: "fixture".into(),
                secret: None,
            },
        )
        .await,
    )?;
    let question = wait(&service, &scope, multi, "awaiting_input")
        .await?
        .pending_input
        .unwrap();
    assert_eq!(question.questions.len(), 2);
    result(
        service
            .answer_input(
                &scope,
                multi,
                InputAnswer {
                    request_id: question.id,
                    answers: BTreeMap::from([
                        ("path".into(), "home/.config".into()),
                        ("mode".into(), "shared".into()),
                    ]),
                },
            )
            .await,
    )?;
    assert!(
        wait(&service, &scope, multi, "complete")
            .await?
            .messages
            .last()
            .unwrap()
            .content
            .contains("已收到全部配置答案")
    );
    result(service.delete(&scope, multi).await)?;
    result(service.delete(&scope, conversation).await)?;
    service.shutdown().await;
    drop(service);
    server.abort();
    std::fs::remove_file(path)?;
    Ok(())
}

#[test]
fn structured_answers_require_every_question_and_valid_choices() {
    let questions:Vec<InputQuestion>=serde_json::from_value(json!([
        {"id":"device","title":"选择设备","options":[{"value":"mac","label":"Mac"}],"allowText":false},
        {"id":"file","title":"文件名","options":[],"allowText":true}
    ])).unwrap();
    assert!(super::input_tool::validate(&questions).is_ok());
    let mut answer = InputAnswer {
        request_id: Uuid::new_v4(),
        answers: BTreeMap::new(),
    };
    assert!(super::input_tool::validate_answers(&questions, &answer).is_err());
    answer.answers.insert("device".into(), "win".into());
    answer.answers.insert("file".into(), "年龄.xlsx".into());
    assert!(super::input_tool::validate_answers(&questions, &answer).is_err());
    answer.answers.insert("device".into(), "mac".into());
    assert!(super::input_tool::validate_answers(&questions, &answer).is_ok());
}
