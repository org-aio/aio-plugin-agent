use super::*;
use anyhow::Result;
use axum::{Json, Router, response::IntoResponse, routing::post};
use serde_json::{Value, json};
use std::sync::Arc;

struct Lookup;

struct DesktopImage;

struct Ask;
#[async_trait::async_trait]
impl Tool for Ask {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"ask","parameters":{"type":"object"}}})
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
async fn paused_desktop_observation_is_sent_after_answer_without_replaying_tools() -> Result<()> {
    let app = Router::new().route("/",post(|Json(body):Json<Value>|async move {
        let messages = body["messages"].as_array().unwrap();
        let values = if messages.is_empty() {
            vec![json!({"choices":[{"delta":{"tool_calls":[
                {"index":0,"id":"observe","type":"function","function":{"name":"desktop","arguments":"{}"}},
                {"index":1,"id":"question","type":"function","function":{"name":"ask","arguments":"{}"}}
            ]},"finish_reason":"tool_calls"}]})]
        } else {
            assert_eq!(messages.len(), 4);
            assert_eq!(messages[1]["tool_call_id"], "observe");
            assert_eq!(messages[2]["tool_call_id"], "question");
            assert_eq!(messages[3]["name"], "device_observation");
            assert_eq!(messages[3]["content"][1]["type"], "image_url");
            vec![json!({"choices":[{"delta":{"content":"已核对恢复后的截图"},"finish_reason":"stop"}]})]
        };
        ([("content-type","text/event-stream")], events(values))
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let request = || reqwest::Client::new().post(format!("http://{address}/"));
    let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(DesktopImage), Arc::new(Ask)];
    let (output, mut receive) = tokio::sync::mpsc::channel(32);
    run(request(), "fixture-vision", vec![], tools.clone(), output).await?;
    let mut waiting = None;
    while let Some(delta) = receive.recv().await {
        if let Delta::Waiting { state, .. } = delta {
            waiting = Some(state);
        }
    }
    let mut state: RunState = serde_json::from_value(serde_json::to_value(waiting.unwrap())?)?;
    assert_eq!(state.observations.len(), 1);
    state.answer(json!({"answer":"继续"}))?;
    let (output, _receive) = tokio::sync::mpsc::channel(32);
    resume(request(), "fixture-vision", state, tools, output).await?;
    server.abort();
    Ok(())
}
#[async_trait::async_trait]
impl Tool for DesktopImage {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"desktop","parameters":{"type":"object"}}})
    }
    async fn invoke(&self, _: Value) -> Result<Value> {
        Ok(json!({"observation":"fresh","image":"data:image/jpeg;base64,/9j/2Q=="}))
    }
    fn take_images(&self, result: &mut Value) -> Vec<String> {
        result
            .as_object_mut()
            .and_then(|value| value.remove("image"))
            .and_then(|value| value.as_str().map(str::to_owned))
            .into_iter()
            .collect()
    }
}

#[tokio::test]
async fn desktop_images_follow_all_tool_receipts_and_old_images_are_released() -> Result<()> {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = calls.clone();
    let app = Router::new().route("/",post(move |Json(body):Json<Value>| {
        let count=count.clone();
        async move {
            let round=count.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
            let messages=body["messages"].as_array().unwrap();
            if round>0 {
                let last=messages.last().unwrap();
                assert_eq!(last["name"],"device_observation");
                assert_eq!(last["content"][1]["type"],"image_url");
                assert_eq!(messages[messages.len()-2]["role"],"tool");
                assert!(!messages[messages.len()-2]["content"].as_str().unwrap().contains("base64"));
                assert_eq!(messages.iter().filter(|m|m["name"]=="device_observation" && m["content"].is_array()).count(),1);
            }
            let values=if round==2 { vec![json!({"choices":[{"delta":{"content":"界面已核对"},"finish_reason":"stop"}]})] }
            else { vec![json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":format!("desktop_{round}"),"function":{"name":"desktop","arguments":"{}"}},{"index":1,"id":format!("lookup_{round}"),"function":{"name":"lookup","arguments":"{\"query\":\"记忆\"}"}}]},"finish_reason":"tool_calls"}]})] };
            ([("content-type","text/event-stream")],events(values))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let (output, _receive) = tokio::sync::mpsc::channel(32);
    run(
        reqwest::Client::new().post(format!("http://{address}/")),
        "fixture-vision",
        vec![],
        vec![Arc::new(DesktopImage), Arc::new(Lookup)],
        output,
    )
    .await?;
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    server.abort();
    Ok(())
}
#[async_trait::async_trait]
impl Tool for Lookup {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"lookup","parameters":{"type":"object"}}})
    }
    async fn invoke(&self, args: Value) -> Result<Value> {
        assert_eq!(args["query"], "记忆");
        Ok(json!({"answer":"已找到"}))
    }
}

fn events(chunks: Vec<Value>) -> String {
    chunks
        .into_iter()
        .map(|value| format!("data: {value}\r\n\r\n"))
        .collect::<String>()
        + "data: [DONE]\r\n\r\n"
}

#[tokio::test]
async fn tool_roundtrip_streams_unicode_and_accumulates_usage() -> Result<()> {
    let app=Router::new().route("/",post(|Json(body):Json<Value>|async move {
        assert_eq!(body["model"],"selected-model");
        let second=body["messages"].as_array().unwrap().iter().any(|m|m["role"]=="tool");
        let values=if second {
            assert_eq!(body["messages"].as_array().unwrap().last().unwrap()["content"],"{\"answer\":\"已找到\"}");
            vec![json!({"choices":[{"delta":{"content":"你好"},"finish_reason":null}]}),json!({"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"total_tokens":7}})]
        } else {vec![
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"lookup","arguments":"{\"query\":"}}]},"finish_reason":null}]}),
            json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"记忆\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"total_tokens":4}})]};
        let bytes=events(values).into_bytes();
        let chunks=bytes.into_iter().map(|b|Ok::<_,std::io::Error>(vec![b]));
        ([("content-type","text/event-stream")],axum::body::Body::from_stream(futures_util::stream::iter(chunks))).into_response()
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let (output, mut receive) = tokio::sync::mpsc::channel(32);
    run(
        reqwest::Client::new().post(format!("http://{address}/")),
        "selected-model",
        vec![json!({"role":"user","content":"检索"})],
        vec![Arc::new(Lookup)],
        output,
    )
    .await?;
    let mut text = String::new();
    let mut tokens = 0;
    while let Some(delta) = receive.recv().await {
        match delta {
            Delta::Text(value) => text.push_str(&value),
            Delta::Tokens(value) => tokens = value,
            Delta::Waiting { .. } => panic!("此测试不应等待用户"),
        }
    }
    assert_eq!(text, "你好");
    assert_eq!(tokens, 11);
    server.abort();
    Ok(())
}

#[tokio::test]
async fn rejects_truncated_stream_and_unauthorized_tool() -> Result<()> {
    for (body,message) in [
        ("data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n".to_owned(),"模型流提前中断"),
        (events(vec![json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"x","function":{"name":"delete_files","arguments":"{}"}}]},"finish_reason":"tool_calls"}]})]),"模型请求了未授权工具")
    ] {
        let app=Router::new().route("/",post(move ||{let body=body.clone();async move{([( "content-type","text/event-stream")],body)}}));
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;let address=listener.local_addr()?;
        let server=tokio::spawn(async move{axum::serve(listener,app).await});
        let (output,_receive)=tokio::sync::mpsc::channel(32);
        let error=run(reqwest::Client::new().post(format!("http://{address}/")),"model",vec![],vec![],output).await.err().unwrap();
        assert!(error.to_string().contains(message),"{error}");server.abort();
    }
    Ok(())
}

struct DeviceList(std::sync::atomic::AtomicUsize);
#[async_trait::async_trait]
impl Tool for DeviceList {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"devices","parameters":{"type":"object","properties":{},"additionalProperties":false}}})
    }
    async fn invoke(&self, args: Value) -> Result<Value> {
        assert_eq!(args, json!({}));
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(json!({"devices":[]}))
    }
}

#[tokio::test]
async fn empty_argument_tool_executes_but_malformed_json_never_does() -> Result<()> {
    for raw in ["", "{invalid}"] {
        let app=Router::new().route("/",post(move |Json(body):Json<Value>|async move {
            let second=body["messages"].as_array().unwrap().iter().any(|m|m["role"]=="tool");
            let values=if second {
                let result:Value=serde_json::from_str(body["messages"].as_array().unwrap().last().unwrap()["content"].as_str().unwrap()).unwrap();
                assert_eq!(result.get("error").is_some(), !raw.is_empty());
                vec![json!({"choices":[{"delta":{"content":"已检查"},"finish_reason":"stop"}]})]
            } else {vec![json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_list","function":{"name":"devices","arguments":raw}}]},"finish_reason":"tool_calls"}]})]};
            ([("content-type","text/event-stream")],events(values))
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let tool = Arc::new(DeviceList(std::sync::atomic::AtomicUsize::new(0)));
        let (output, _receive) = tokio::sync::mpsc::channel(32);
        run(
            reqwest::Client::new().post(format!("http://{address}/")),
            "model",
            vec![],
            vec![tool.clone()],
            output,
        )
        .await?;
        assert_eq!(
            tool.0.load(std::sync::atomic::Ordering::SeqCst),
            usize::from(raw.is_empty())
        );
        server.abort();
    }
    Ok(())
}
