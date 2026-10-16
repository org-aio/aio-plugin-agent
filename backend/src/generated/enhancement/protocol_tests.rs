use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::post,
};
use rmcp::{
    ServiceExt,
    model::{CallToolRequest, CallToolRequestParams, ClientRequest},
    service::PeerRequestOptions,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use tokio::sync::Notify;
use uuid::Uuid;

use super::{configuration::Binding, controller, service::EnhancementService, tests::request};
use crate::{
    configuration::{Gateway, RuntimeConfig},
    generated::conversation::{
        model::{ProviderDraft, Scope},
        service::AgentService,
        service_impl::AgentServiceImpl,
    },
};

#[derive(Clone, Default)]
struct Fixture {
    calls: Arc<AtomicUsize>,
    meters: Arc<AtomicUsize>,
    release: Arc<Notify>,
}

fn frame(event: Value) -> String {
    format!("data: {event}\n\n")
}

async fn generate(
    State(fixture): State<Fixture>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    assert_eq!(
        headers["x-aio-endpoint"],
        "https://model.fixture.invalid/v1"
    );
    assert_eq!(headers["x-aio-token"], "broker-fixture");
    assert_eq!(headers["authorization"], "Bearer provider-secret-fixture");
    assert_eq!(body["model"], "source-fixture");
    assert_eq!(body["store"], false);
    assert!(body.get("tools").is_none());
    assert_eq!(body["input"][0]["role"], "developer");
    let input: Value = serde_json::from_str(body["input"][1]["content"].as_str().unwrap_or("{}"))
        .unwrap_or(Value::Null);
    let requirement = input["requirement"].as_str().unwrap_or("");
    fixture.calls.fetch_add(1, Ordering::SeqCst);
    let source = if input["target"]["language"] == "rhai" {
        "fn invoke(input, context) { input * 2 }"
    } else {
        "export function invoke(input: number) { return input * 2; }"
    };
    let text = match requirement {
        "malformed" => "```json\n{}\n```".into(),
        "empty" => json!({"source":" "}).to_string(),
        "escalate" => json!({"source":source, "tenant_id":"foreign"}).to_string(),
        "huge" => json!({"source":"a".repeat(66000)}).to_string(),
        _ => json!({"source":source}).to_string(),
    };
    let output = if requirement == "tool" {
        json!({"type":"function_call","call_id":"forbidden","name":"activate","arguments":"{}"})
    } else {
        json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]})
    };
    let wire = match requirement {
        "incomplete" => {
            frame(json!({"type":"response.incomplete","response":{"status":"incomplete"}}))
        }
        "truncated" => frame(json!({"type":"response.output_text.delta","delta":text})),
        "tool" => frame(
            json!({"type":"response.completed","response":{"status":"completed","output":[output],"usage":{"total_tokens":7}}}),
        ),
        _ => {
            let middle = text.len() / 2;
            let mut wire =
                frame(json!({"type":"response.output_text.delta","delta":&text[..middle]}));
            wire.push_str(&frame(
                json!({"type":"response.output_text.delta","delta":&text[middle..]}),
            ));
            wire.push_str(&frame(json!({"type":"response.completed","response":{"status":"completed","output":[output],"usage":{"total_tokens":7}}})));
            wire
        }
    };
    let wait = requirement == "wait";
    let stream = futures_util::stream::once(async move {
        if wait {
            fixture.release.notified().await;
        }
        Ok::<_, std::convert::Infallible>(wire)
    });
    (
        [("content-type", "text/event-stream")],
        Body::from_stream(stream),
    )
        .into_response()
}

async fn meter(State(fixture): State<Fixture>, Json(body): Json<Value>) -> Json<Value> {
    assert_eq!(body["tenantId"], "model-tenant");
    assert_eq!(body["userId"], "model-owner");
    assert_eq!(body["quantity"], 7);
    fixture.meters.fetch_add(1, Ordering::SeqCst);
    Json(json!({"status":200}))
}

async fn until(mut condition: impl FnMut() -> bool) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(1), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("等待测试条件超时")
}

#[tokio::test]
#[ignore = "需要 scripts/setup-dev.mjs 创建的隔离 AIO_INPUT_TEST_CONFIG"]
async fn mcp_generation_uses_live_bound_model_and_cancels_without_escalation() -> Result<()> {
    let config: Value = serde_json::from_str(&std::fs::read_to_string(std::env::var(
        "AIO_INPUT_TEST_CONFIG",
    )?)?)?;
    let fixture = Fixture::default();
    let socket = std::env::temp_dir().join(format!("aio-model-{}.sock", Uuid::new_v4().simple()));
    let broker_listener = tokio::net::UnixListener::bind(&socket)?;
    let broker = Router::new()
        .route("/egress/responses", post(generate))
        .route("/meter", post(meter))
        .with_state(fixture.clone());
    let broker_task = tokio::spawn(async move { axum::serve(broker_listener, broker).await });
    let settings = RuntimeConfig {
        database_url: config["databaseUrl"]
            .as_str()
            .context("缺少隔离数据库")?
            .into(),
        encryption_key: [43; 32],
        gateway: Some(Gateway {
            socket: socket.clone(),
            token: "broker-fixture".into(),
            worker_capabilities: vec![],
        }),
        allowed_endpoints: ["https://model.fixture.invalid/v1".into()].into(),
        generation_timeout: Duration::from_secs(2),
        allow_loopback: false,
        memory: None,
    };
    let implementation = Arc::new(AgentServiceImpl::connect(settings).await?);
    let scope = Scope {
        tenant: "model-tenant".into(),
        user: "model-owner".into(),
        context_id: None,
    };
    let provider = implementation
        .save_provider(
            &scope,
            None,
            ProviderDraft {
                label: "source-fixture".into(),
                endpoint: "https://model.fixture.invalid/v1".into(),
                model: "source-fixture".into(),
                secret: Some("provider-secret-fixture".into()),
            },
        )
        .await
        .map_err(|error| anyhow::anyhow!("{}", error.1))?
        .id;
    let stored: Vec<u8> = sqlx::query_scalar("SELECT secret FROM agent_providers WHERE id=$1")
        .bind(provider)
        .fetch_one(&implementation.core.pool)
        .await?;
    assert!(!String::from_utf8_lossy(&stored).contains("provider-secret-fixture"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let token = "model-access-fixture-0000000000000000";
    let router = controller::router(
        implementation.clone(),
        Binding {
            tenant_id: scope.tenant.clone(),
            user_id: scope.user.clone(),
            provider_id: provider,
            token: token.into(),
            public_origin: origin.clone(),
        },
    )?;
    let mcp_task = tokio::spawn(async move { axum::serve(listener, router).await });
    let endpoint = format!("{origin}/mcp/enhancements");
    let http = reqwest::Client::builder().no_proxy().build()?;
    assert_eq!(http.post(&endpoint).send().await?.status(), 401);
    assert_eq!(
        http.post(&endpoint)
            .bearer_auth("wrong-token")
            .send()
            .await?
            .status(),
        401
    );
    assert_eq!(
        http.post(&endpoint)
            .bearer_auth(token)
            .header("origin", "https://foreign.invalid")
            .send()
            .await?
            .status(),
        403
    );
    let mut transport = StreamableHttpClientTransportConfig::with_uri(endpoint).auth_header(token);
    transport
        .custom_headers
        .insert("x-aio-user-id".parse()?, "foreign-user".parse()?);
    transport
        .custom_headers
        .insert("x-aio-tenant-id".parse()?, "foreign-tenant".parse()?);
    let mut client = ().serve(StreamableHttpClientTransport::from_config(transport)).await?;
    let tools = client.list_all_tools().await?;
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "enhancement_source");
    assert_eq!(
        tools[0]
            .annotations
            .as_ref()
            .and_then(|value| value.read_only_hint),
        Some(false)
    );
    assert_eq!(
        tools[0]
            .annotations
            .as_ref()
            .and_then(|value| value.idempotent_hint),
        Some(false)
    );
    assert!(tools[0].output_schema.is_some());
    for language in ["rhai", "typescript"] {
        let arguments = serde_json::to_value(request(language, "double")?)?
            .as_object()
            .context("输入不是对象")?
            .clone();
        let result = client
            .call_tool(CallToolRequestParams::new("enhancement_source").with_arguments(arguments))
            .await?;
        assert_ne!(result.is_error, Some(true));
        let value = result.structured_content.context("没有源码")?;
        assert_eq!(value.as_object().context("输出不是对象")?.len(), 1);
        assert!(
            value["source"]
                .as_str()
                .is_some_and(|source| source.contains("input * 2"))
        );
    }
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.meters.load(Ordering::SeqCst), 2);

    let all_permits = implementation.core.quota.acquire_many(4).await?;
    assert!(
        implementation
            .enhancement_source(&scope, provider, request("rhai", "double")?)
            .await
            .is_err()
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 2);
    drop(all_permits);

    for foreign in [
        Scope {
            user: "foreign".into(),
            ..scope.clone()
        },
        Scope {
            tenant: "foreign".into(),
            ..scope.clone()
        },
    ] {
        assert!(
            implementation
                .enhancement_source(&foreign, provider, request("rhai", "double")?)
                .await
                .is_err()
        );
    }
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 2);
    for invalid in [
        "malformed",
        "empty",
        "escalate",
        "huge",
        "incomplete",
        "truncated",
        "tool",
    ] {
        let before = fixture.calls.load(Ordering::SeqCst);
        assert!(
            implementation
                .enhancement_source(&scope, provider, request("rhai", invalid)?)
                .await
                .is_err(),
            "{invalid}"
        );
        assert_eq!(
            fixture.calls.load(Ordering::SeqCst),
            before + 1,
            "不能重试 {invalid}"
        );
        assert_eq!(implementation.core.quota.available_permits(), 4);
    }
    let before = fixture.calls.load(Ordering::SeqCst);
    assert!(
        implementation
            .enhancement_source(&scope, provider, request("rhai", "wait")?)
            .await
            .is_err()
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), before + 1);
    assert_eq!(implementation.core.quota.available_permits(), 4);

    // 真正的 MCP 取消必须释放同一模型 Future，而不是在客户端返回后继续生成。
    let before = fixture.calls.load(Ordering::SeqCst);
    let arguments = serde_json::to_value(request("rhai", "wait")?)?
        .as_object()
        .context("输入不是对象")?
        .clone();
    let operation = ClientRequest::CallToolRequest(CallToolRequest::new(
        CallToolRequestParams::new("enhancement_source").with_arguments(arguments),
    ));
    let pending = client
        .peer()
        .send_cancellable_request(operation, PeerRequestOptions::no_options())
        .await?;
    until(|| fixture.calls.load(Ordering::SeqCst) > before).await?;
    assert_eq!(implementation.core.quota.available_permits(), 3);
    pending.cancel(Some("取消生成".into())).await?;
    until(|| implementation.core.quota.available_permits() == 4).await?;

    let before = fixture.calls.load(Ordering::SeqCst);
    let waiting = implementation.enhancement_source(&scope, provider, request("rhai", "wait")?);
    let reconfigure = async {
        until(|| fixture.calls.load(Ordering::SeqCst) > before).await?;
        sqlx::query("UPDATE agent_providers SET model='changed-model' WHERE id=$1")
            .bind(provider)
            .execute(&implementation.core.pool)
            .await?;
        fixture.release.notify_waiters();
        Ok::<_, anyhow::Error>(())
    };
    let (generated, reconfigured) = tokio::join!(waiting, reconfigure);
    reconfigured?;
    assert!(generated.is_err());
    sqlx::query("UPDATE agent_providers SET model='source-fixture' WHERE id=$1")
        .bind(provider)
        .execute(&implementation.core.pool)
        .await?;

    let before = fixture.calls.load(Ordering::SeqCst);
    let waiting = implementation.enhancement_source(&scope, provider, request("rhai", "wait")?);
    let revoke = async {
        until(|| fixture.calls.load(Ordering::SeqCst) > before).await?;
        sqlx::query("DELETE FROM agent_providers WHERE id=$1")
            .bind(provider)
            .execute(&implementation.core.pool)
            .await?;
        fixture.release.notify_waiters();
        Ok::<_, anyhow::Error>(())
    };
    let (generated, revoked) = tokio::join!(waiting, revoke);
    revoked?;
    assert!(generated.is_err());
    assert_eq!(implementation.core.quota.available_permits(), 4);
    let calls = fixture.calls.load(Ordering::SeqCst);
    assert!(
        implementation
            .enhancement_source(&scope, provider, request("rhai", "double")?)
            .await
            .is_err()
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), calls);
    client.close().await?;
    implementation.shutdown().await;
    assert!(
        implementation
            .enhancement_source(&scope, provider, request("rhai", "double")?)
            .await
            .is_err()
    );
    mcp_task.abort();
    broker_task.abort();
    std::fs::remove_file(socket)?;
    Ok(())
}
