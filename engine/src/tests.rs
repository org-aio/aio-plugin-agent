use super::*;
use anyhow::{Context, Result};
use axum::{Json, Router, response::IntoResponse, routing::post};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub(crate) fn text_item(text: &str) -> Value {
    json!({"type":"message","id":"msg_test","role":"assistant","status":"completed","content":[{"type":"output_text","text":text,"annotations":[]}],"phase":"final_answer"})
}
pub(crate) fn call_item(id: &str, name: &str, arguments: &str) -> Value {
    json!({"type":"function_call","id":format!("fc_{id}"),"call_id":id,"name":name,"arguments":arguments,"status":"completed"})
}
pub(crate) fn events(items: Vec<Value>, tokens: i64) -> String {
    format!(
        "event: response.completed\r\ndata: {}\r\n\r\n",
        json!({"type":"response.completed","response":{"status":"completed","output":items,"usage":{"total_tokens":tokens}}})
    )
}
async fn server(app: Router) -> Result<(String, tokio::task::JoinHandle<std::io::Result<()>>)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/responses", listener.local_addr()?);
    Ok((
        url,
        tokio::spawn(async move { axum::serve(listener, app).await }),
    ))
}
struct Device(Arc<AtomicUsize>);
#[async_trait::async_trait]
impl Tool for Device {
    fn definition(&self) -> Value {
        json!({"type":"function","strict":false,"name":"desktop","parameters":{"type":"object","properties":{},"additionalProperties":false}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        assert_eq!(arguments, json!({}));
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"observation":"fresh","image":"data:image/jpeg;base64,/9j/2Q=="}))
    }
    fn take_images(&self, result: &mut Value) -> Vec<String> {
        result
            .as_object_mut()
            .and_then(|v| v.remove("image"))
            .and_then(|v| v.as_str().map(str::to_owned))
            .into_iter()
            .collect()
    }
}
struct Ask;
#[async_trait::async_trait]
impl Tool for Ask {
    fn definition(&self) -> Value {
        json!({"type":"function","strict":false,"name":"ask","parameters":{"type":"object"}})
    }
    async fn invoke(&self, _: Value) -> Result<Value> {
        Err(InputRequired {
            request: json!({"question":"继续？"}),
            retry: false,
        }
        .into())
    }
}

#[tokio::test]
async fn fragmented_responses_preserve_reasoning_tools_images_and_usage() -> Result<()> {
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let app=Router::new().route("/responses",post(move |Json(body):Json<Value>| {
        let count=count.clone();
        async move {
            assert_eq!(body["model"],"selected-model");
            assert_eq!(body["stream"],true);
            assert_eq!(body["store"],false);
            assert_eq!(body["include"],json!(["reasoning.encrypted_content"]));
            assert!(body.get("messages").is_none() && body.get("stream_options").is_none());
            assert_eq!(body["tools"][0]["name"],"desktop");
            assert_eq!(body["tools"][0]["strict"],false);
            assert!(body["tools"][0].get("function").is_none());
            let round=count.fetch_add(1,Ordering::SeqCst);
            let items=body["input"].as_array().expect("输入项");
            let frames=if round==0 {
                let reasoning=json!({"type":"reasoning","id":"rs_test","summary":[],"encrypted_content":"opaque-proof"});
                let call=call_item("observe","desktop","{}");
                let mut frames=String::new();
                for event in [
                    json!({"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","id":"fc_observe","call_id":"observe","name":"desktop","arguments":""}}),
                    json!({"type":"response.function_call_arguments.delta","output_index":1,"item_id":"fc_observe","delta":"{"}),
                    json!({"type":"response.function_call_arguments.delta","output_index":1,"item_id":"fc_observe","delta":"}"}),
                    json!({"type":"response.output_item.done","output_index":1,"item":call})
                ] {frames.push_str(&format!("data: {event}\r\n\r\n"));}
                frames+&events(vec![reasoning,call],4)
            } else {
                assert_eq!(items[1]["encrypted_content"],"opaque-proof");
                assert_eq!(items[2]["call_id"],"observe");
                assert_eq!(items[3]["type"],"function_call_output");
                assert_eq!(items[3]["call_id"],"observe");
                assert!(!items[3]["output"].as_str().expect("回执").contains("base64"));
                assert_eq!(items[4]["content"][1]["type"],"input_image");
                assert!(items[4].get("name").is_none());
                format!("data: {}\r\n\r\n{}",json!({"type":"response.output_text.delta","delta":"你好"}),events(vec![text_item("你好，已核对截图")],7))
            };
            let chunks=frames.into_bytes().into_iter().map(|byte|Ok::<_,std::io::Error>(vec![byte]));
            ([("content-type","text/event-stream")],axum::body::Body::from_stream(futures_util::stream::iter(chunks))).into_response()
        }
    }));
    let (url, server) = server(app).await?;
    let effects = Arc::new(AtomicUsize::new(0));
    let (send, mut receive) = tokio::sync::mpsc::channel(32);
    run(
        reqwest::Client::new().post(url),
        "selected-model",
        vec![json!({"role":"user","content":"观察桌面"})],
        vec![Arc::new(Device(effects.clone()))],
        send,
    )
    .await?;
    let mut text = String::new();
    let mut tokens = 0;
    while let Some(delta) = receive.recv().await {
        match delta {
            Delta::Text(value) => text.push_str(&value),
            Delta::Tokens(value) => tokens = value,
            Delta::Waiting { .. } => anyhow::bail!("不应暂停"),
        }
    }
    assert_eq!(text, "你好，已核对截图");
    assert_eq!(tokens, 11);
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    server.abort();
    Ok(())
}

#[tokio::test]
async fn pause_and_restore_keep_images_and_never_replay_effects() -> Result<()> {
    let app = Router::new().route(
        "/responses",
        post(|Json(body): Json<Value>| async move {
            let items = body["input"].as_array().expect("输入项");
            let output = if items.is_empty() {
                vec![
                    call_item("observe", "desktop", "{}"),
                    call_item("question", "ask", "{}"),
                ]
            } else {
                assert_eq!(items.len(), 5);
                assert_eq!(items[2]["call_id"], "observe");
                assert_eq!(items[3]["call_id"], "question");
                assert_eq!(items[4]["content"][1]["type"], "input_image");
                vec![text_item("恢复完成")]
            };
            ([("content-type", "text/event-stream")], events(output, 1))
        }),
    );
    let (url, server) = server(app).await?;
    let effects = Arc::new(AtomicUsize::new(0));
    let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(Device(effects.clone())), Arc::new(Ask)];
    let (send, mut receive) = tokio::sync::mpsc::channel(32);
    run(
        reqwest::Client::new().post(&url),
        "fixture",
        vec![],
        tools.clone(),
        send,
    )
    .await?;
    let mut waiting = None;
    while let Some(delta) = receive.recv().await {
        if let Delta::Waiting { state, .. } = delta {
            waiting = Some(state);
        }
    }
    let mut state: RunState =
        serde_json::from_value(serde_json::to_value(waiting.context("未暂停")?)?)?;
    state.answer(json!({"answer":"继续"}))?;
    let (send, _receive) = tokio::sync::mpsc::channel(32);
    resume(
        reqwest::Client::new().post(&url),
        "fixture",
        state,
        tools,
        send,
    )
    .await?;
    assert_eq!(effects.load(Ordering::SeqCst), 1);
    server.abort();
    Ok(())
}

#[tokio::test]
async fn rejects_failed_truncated_and_unauthorized_calls_before_effects() -> Result<()> {
    for body in [
        "data: [DONE]\n\n".into(),
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n".into(),
        "data: {invalid}\n\n".into(),
        events(vec![call_item("x", "delete_files", "{}")], 0),
        events(
            vec![
                call_item("x", "desktop", "{}"),
                call_item("x", "desktop", "{}"),
            ],
            0,
        ),
        events(
            vec![
                call_item("x", "desktop", "{}"),
                call_item("y", "delete_files", "{}"),
            ],
            0,
        ),
        "data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\"}}\n\n"
            .into(),
        "data: {\"type\":\"error\",\"message\":\"private upstream secret\"}\n\n".into(),
    ] {
        let app = Router::new().route(
            "/responses",
            post(move || {
                let body = body.clone();
                async move { ([("content-type", "text/event-stream")], body) }
            }),
        );
        let (url, server) = server(app).await?;
        let effects = Arc::new(AtomicUsize::new(0));
        let (send, _receive) = tokio::sync::mpsc::channel(32);
        let error = run(
            reqwest::Client::new().post(url),
            "fixture",
            vec![],
            vec![Arc::new(Device(effects.clone()))],
            send,
        )
        .await
        .err()
        .context("应拒绝无效回复")?;
        assert!(!error.to_string().contains("private upstream"));
        assert_eq!(effects.load(Ordering::SeqCst), 0);
        server.abort();
    }
    Ok(())
}

#[tokio::test]
async fn desktop_fallback_preserves_user_images_and_releases_stale_observations() -> Result<()> {
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    let app = Router::new().route(
        "/responses",
        post(move |Json(body): Json<Value>| {
            let requests = requests.clone();
            async move {
                let round = requests.fetch_add(1, Ordering::SeqCst);
                let items = body["input"].as_array().expect("输入项");
                assert_eq!(items[0]["content"][0]["type"], "input_image");
                if round == 1 {
                    return (axum::http::StatusCode::BAD_REQUEST, "unsupported image")
                        .into_response();
                }
                if round > 1 {
                    assert!(
                        items.last().expect("观察")["content"]
                            .as_str()
                            .expect("文字观察")
                            .contains("禁止声称已看图或猜测坐标")
                    );
                }
                let output = if round == 3 {
                    vec![text_item("完成")]
                } else {
                    vec![call_item(&format!("observe_{round}"), "desktop", "{}")]
                };
                ([("content-type", "text/event-stream")], events(output, 1)).into_response()
            }
        }),
    );
    let (url, server) = server(app).await?;
    let effects = Arc::new(AtomicUsize::new(0));
    let (send, _receive) = tokio::sync::mpsc::channel(32);
    run(reqwest::Client::new().post(url),"fixture",vec![json!({"role":"user","content":[{"type":"input_image","image_url":"user-supplied-image"}]})],vec![Arc::new(Device(effects.clone()))],send).await?;
    assert_eq!(count.load(Ordering::SeqCst), 4);
    assert_eq!(effects.load(Ordering::SeqCst), 2);
    server.abort();
    Ok(())
}

#[tokio::test]
async fn desktop_fallback_is_bounded_and_never_retries_authentication_or_text_errors() -> Result<()>
{
    for (status, image, expected) in [
        (400, true, 2),
        (401, true, 1),
        (403, true, 1),
        (400, false, 1),
    ] {
        let count = Arc::new(AtomicUsize::new(0));
        let requests = count.clone();
        let app = Router::new().route(
            "/responses",
            post(move || {
                requests.fetch_add(1, Ordering::SeqCst);
                async move {
                    (
                        axum::http::StatusCode::from_u16(status).expect("状态码"),
                        "private detail",
                    )
                }
            }),
        );
        let (url, server) = server(app).await?;
        let mut state = RunState::new(vec![]);
        if image {
            state
                .observations
                .push(json!({"type":"input_image","image_url":"data:image/jpeg;base64,/9j/2Q=="}));
        }
        let (send, _receive) = tokio::sync::mpsc::channel(32);
        assert!(
            resume(
                reqwest::Client::new().post(url),
                "fixture",
                state,
                vec![],
                send
            )
            .await
            .is_err()
        );
        assert_eq!(count.load(Ordering::SeqCst), expected);
        server.abort();
    }
    Ok(())
}

#[tokio::test]
async fn empty_arguments_execute_but_malformed_arguments_only_return_error() -> Result<()> {
    for raw in ["", "{invalid}"] {
        let app = Router::new().route(
            "/responses",
            post(move |Json(body): Json<Value>| async move {
                let items = body["input"].as_array().expect("输入项");
                let output = if let Some(receipt) =
                    items.iter().find(|v| v["type"] == "function_call_output")
                {
                    let value: Value =
                        serde_json::from_str(receipt["output"].as_str().expect("回执"))
                            .expect("JSON 回执");
                    assert_eq!(value.get("error").is_some(), !raw.is_empty());
                    vec![text_item("完成")]
                } else {
                    vec![call_item("x", "desktop", raw)]
                };
                ([("content-type", "text/event-stream")], events(output, 1))
            }),
        );
        let (url, server) = server(app).await?;
        let effects = Arc::new(AtomicUsize::new(0));
        let (send, _receive) = tokio::sync::mpsc::channel(32);
        run(
            reqwest::Client::new().post(url),
            "fixture",
            vec![],
            vec![Arc::new(Device(effects.clone()))],
            send,
        )
        .await?;
        assert_eq!(effects.load(Ordering::SeqCst), usize::from(raw.is_empty()));
        server.abort();
    }
    Ok(())
}

#[tokio::test]
async fn cancelling_a_pending_stream_does_not_execute_a_partial_tool() -> Result<()> {
    let started = Arc::new(tokio::sync::Notify::new());
    let received = started.clone();
    let app=Router::new().route("/responses",post(move || {let received=received.clone();async move {
        received.notify_one();
        let first=Ok::<_,std::io::Error>("data: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"call_id\":\"x\",\"name\":\"desktop\",\"arguments\":\"{}\"}}\n\n");
        let stream=futures_util::StreamExt::chain(futures_util::stream::iter([first]),futures_util::stream::pending());
        ([("content-type","text/event-stream")],axum::body::Body::from_stream(stream))
    }}));
    let (url, server) = server(app).await?;
    let effects = Arc::new(AtomicUsize::new(0));
    let (send, _receive) = tokio::sync::mpsc::channel(32);
    let observed = effects.clone();
    let task = tokio::spawn(async move {
        run(
            reqwest::Client::new().post(url),
            "fixture",
            vec![],
            vec![Arc::new(Device(observed))],
            send,
        )
        .await
    });
    started.notified().await;
    task.abort();
    assert!(task.await.is_err());
    assert_eq!(effects.load(Ordering::SeqCst), 0);
    server.abort();
    Ok(())
}

#[test]
fn upgrades_old_checkpoint_images_once_without_losing_tool_position() -> Result<()> {
    let old = json!({"messages":[{"role":"assistant","tool_calls":[{"id":"x","type":"function","function":{"name":"desktop","arguments":"{}"}}]},{"role":"tool","tool_call_id":"x","content":"saved"},{"role":"user","name":"device_observation","content":[{"type":"text","text":"界面"},{"type":"image_url","image_url":{"url":"data:image/png;base64,aA=="}}]}],"pending":[],"rounds_left":5,"tokens":12,"observations":[{"type":"image_url","image_url":{"url":"data:image/png;base64,aQ=="}}]});
    let mut state: RunState = serde_json::from_value(old)?;
    crate::checkpoint::upgrade(&mut state)?;
    assert_eq!(state.messages[0]["call_id"], "x");
    assert_eq!(state.messages[1]["output"], "saved");
    assert_eq!(state.messages[2]["content"][1]["type"], "input_image");
    assert_eq!(
        state.observations[0]["image_url"],
        "data:image/png;base64,aQ=="
    );
    assert_eq!(state.observation_indices, vec![2]);
    assert_eq!((state.rounds_left, state.tokens), (5, 12));
    let upgraded = serde_json::to_value(&state)?;
    crate::checkpoint::upgrade(&mut state)?;
    assert_eq!(serde_json::to_value(&state)?, upgraded);
    Ok(())
}

#[tokio::test]
async fn only_latest_desktop_image_is_replayed_on_later_rounds() -> Result<()> {
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let app = Router::new().route(
        "/responses",
        post(move |Json(body): Json<Value>| {
            let count = count.clone();
            async move {
                let round = count.fetch_add(1, Ordering::SeqCst);
                let items = body["input"].as_array().expect("输入项");
                if round > 0 {
                    assert_eq!(
                        items
                            .iter()
                            .filter(|item| item["content"].is_array())
                            .count(),
                        1
                    );
                    assert_eq!(
                        items.last().expect("观察")["content"][1]["type"],
                        "input_image"
                    );
                }
                if round == 2 {
                    assert!(items.iter().any(
                            |item| item["content"] == "旧桌面截图已释放，请依据最新观察操作。"
                        ));
                }
                let output = if round == 2 {
                    vec![text_item("完成")]
                } else {
                    vec![call_item(&format!("x_{round}"), "desktop", "{}")]
                };
                ([("content-type", "text/event-stream")], events(output, 1))
            }
        }),
    );
    let (url, server) = server(app).await?;
    let effects = Arc::new(AtomicUsize::new(0));
    let (send, _receive) = tokio::sync::mpsc::channel(32);
    run(
        reqwest::Client::new().post(url),
        "fixture",
        vec![],
        vec![Arc::new(Device(effects.clone()))],
        send,
    )
    .await?;
    assert_eq!(effects.load(Ordering::SeqCst), 2);
    server.abort();
    Ok(())
}
