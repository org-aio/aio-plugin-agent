use super::*;
use anyhow::Result;
use axum::{Json, Router, routing::post};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Effect(Arc<AtomicUsize>);
#[async_trait::async_trait]
impl Tool for Effect {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"effect","parameters":{"type":"object"}}})
    }
    async fn invoke(&self, _: Value) -> Result<Value> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"done":true}))
    }
}
struct Ask;
#[async_trait::async_trait]
impl Tool for Ask {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"ask","parameters":{"type":"object"}}})
    }
    async fn invoke(&self, _: Value) -> Result<Value> {
        Err(InputRequired {
            request: json!({"questions":[{"id":"device","title":"哪台电脑？"}]}),
            retry: false,
        }
        .into())
    }
}

#[tokio::test]
async fn checkpoint_resumes_remaining_calls_without_replaying_completed_effects() -> Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(AtomicUsize::new(0));
    let received = observed.clone();
    let router=Router::new().route("/",post(move |Json(body):Json<Value>| {
        let received=received.clone();async move {
            received.fetch_add(1,Ordering::SeqCst);
            let messages=body["messages"].as_array().unwrap();
            assert_eq!(messages.iter().filter(|m|m["role"]=="tool").count(),3);
            assert!(messages.iter().any(|m|m["tool_call_id"]=="q" && m["content"].as_str().unwrap().contains("Mac mini")));
            ([("content-type","text/event-stream")],"data: {\"choices\":[{\"delta\":{\"content\":\"完成\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/", listener.local_addr()?);
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    let pending: Vec<ToolCall> = serde_json::from_value(json!([
        {"id":"before","type":"function","function":{"name":"effect","arguments":"{}"}},
        {"id":"q","type":"function","function":{"name":"ask","arguments":"{}"}},
        {"id":"after","type":"function","function":{"name":"effect","arguments":"{}"}}
    ]))?;
    let mut state = RunState::new(vec![json!({"role":"assistant","tool_calls":pending})]);
    state.pending = pending;
    let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(Effect(calls.clone())), Arc::new(Ask)];
    let (send, mut receive) = tokio::sync::mpsc::channel(32);
    resume(
        reqwest::Client::new().post(&url),
        "fixture",
        state,
        tools.clone(),
        send,
    )
    .await?;
    let Some(Delta::Waiting { state, input }) = receive.recv().await else {
        anyhow::bail!("没有暂停");
    };
    assert!(!input.retry);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(observed.load(Ordering::SeqCst), 0);
    let mut restored: RunState = serde_json::from_slice(&serde_json::to_vec(&state)?)?;
    restored.answer(json!({"device":"Mac mini"}))?;
    let (send, mut receive) = tokio::sync::mpsc::channel(32);
    resume(
        reqwest::Client::new().post(&url),
        "fixture",
        restored,
        tools,
        send,
    )
    .await?;
    let mut text = String::new();
    while let Some(delta) = receive.recv().await {
        if let Delta::Text(value) = delta {
            text.push_str(&value);
        }
    }
    assert_eq!(text, "完成");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    server.abort();
    Ok(())
}
