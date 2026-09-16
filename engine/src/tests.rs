use super::*;
use anyhow::Result;
use axum::{Json, Router, response::IntoResponse, routing::post};
use serde_json::{Value, json};
use std::sync::Arc;

struct Lookup;
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
