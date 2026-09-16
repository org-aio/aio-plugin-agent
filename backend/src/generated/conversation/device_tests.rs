use super::*;
use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};
use std::sync::Mutex;

#[derive(Clone)]
struct Fixture {
    requests: Arc<Mutex<Vec<Value>>>,
    outcome: &'static str,
}

async fn handler(
    State(fixture): State<Fixture>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    assert_eq!(headers["x-aio-token"], "test-token");
    assert_eq!(body["tenantId"], "tenant");
    assert_eq!(body["userId"], "owner");
    fixture.requests.lock().unwrap().push(body.clone());
    Json(match body["operation"].as_str().unwrap() {
        "list" => json!([{ "id":Uuid::nil(), "status":"online"}]),
        "openApp" => json!({"id":body["requestId"],"state":"queued"}),
        "task" => {
            json!({"id":body["taskId"],"state":fixture.outcome,"result":{"running":true,"pid":49802},"error":"测试失败"})
        }
        _ => panic!("意外操作"),
    })
}

async fn fixture(
    outcome: &'static str,
    wait: Duration,
) -> (Arc<Broker>, Fixture, tokio::task::JoinHandle<()>) {
    let state = Fixture {
        requests: Default::default(),
        outcome,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/workers", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/workers", post(handler))
        .with_state(state.clone());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let broker = Arc::new(Broker {
        client: reqwest::Client::builder().no_proxy().build().unwrap(),
        token: "test-token".into(),
        tenant: "tenant".into(),
        user: "owner".into(),
        assistant: Uuid::new_v4(),
        endpoint,
        wait,
        selected: None,
        prompt: String::new(),
    });
    (broker, state, task)
}
fn arguments() -> Value {
    json!({"worker_id":Uuid::nil(),"application":"Postman"})
}

#[tokio::test]
async fn injects_owner_and_polls_until_process_is_confirmed() {
    let (broker, state, server) = fixture("complete", Duration::from_secs(2)).await;
    assert_eq!(
        List(broker.clone()).invoke(json!({})).await.unwrap()[0]["status"],
        "online"
    );
    let tool = Open(broker);
    let result = tool.invoke(arguments()).await.unwrap();
    assert_eq!(result["state"], "complete");
    assert_eq!(result["result"]["pid"], 49802);
    assert_eq!(state.requests.lock().unwrap().len(), 4);
    server.abort();
}

#[tokio::test]
async fn rejects_model_supplied_identity_before_contacting_broker() {
    let (broker, state, server) = fixture("complete", Duration::ZERO).await;
    assert!(
        List(broker.clone())
            .invoke(json!({"userId":"other"}))
            .await
            .is_err()
    );
    let mut args = arguments();
    args["tenantId"] = json!("other");
    assert!(Open(broker).invoke(args).await.is_err());
    assert!(state.requests.lock().unwrap().is_empty());
    server.abort();
}

#[tokio::test]
async fn queued_is_not_success_and_retries_reuse_request_id() {
    let (broker, state, server) = fixture("complete", Duration::ZERO).await;
    let tool = Open(broker);
    for _ in 0..2 {
        let result = tool.invoke(arguments()).await.unwrap();
        assert_eq!(result["state"], "queued");
        assert!(result.get("result").is_none());
    }
    let requests = state.requests.lock().unwrap();
    assert_eq!(requests[1]["requestId"], requests[3]["requestId"]);
    server.abort();
}

#[tokio::test]
async fn failed_device_is_reported_as_failure() {
    let (broker, _, server) = fixture("failed", Duration::from_secs(2)).await;
    let result = Open(broker).invoke(arguments()).await.unwrap();
    assert_eq!(result["state"], "failed");
    assert!(result.get("result").is_none());
    server.abort();
}
