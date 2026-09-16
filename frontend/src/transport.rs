use az_agent_model::*;
use dioxus::prelude::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use uuid::Uuid;
pub async fn request<T: DeserializeOwned>(
    method: &str,
    path: &str,
    body: Value,
) -> Result<T, String> {
    let mut bridge = document::eval(
        r#"
        const [method, path, body] = await dioxus.recv();
        try { dioxus.send({data: (await window.aioPlugin.json(method, path, body ?? undefined)) ?? null}); }
        catch(error) { dioxus.send({error: String(error.message || '请求失败')}); }
    "#,
    );
    bridge
        .send((method, path, body))
        .map_err(|e| e.to_string())?;
    let response: Value = bridge.recv().await.map_err(|e| e.to_string())?;
    if let Some(error) = response["error"].as_str() {
        return Err(error.to_owned());
    }
    serde_json::from_value(response["data"].clone()).map_err(|e| e.to_string())
}
pub async fn memory<T: DeserializeOwned>(
    method: &str,
    path: &str,
    body: Value,
) -> Result<T, String> {
    request(
        "POST",
        "/memory",
        json!({ "method" : method, "path" : path, "body" : body }),
    )
    .await
}
pub async fn get_thread(id: Uuid) -> Result<Thread, String> {
    request("GET", &format!("/conversations/{id}"), Value::Null).await
}
pub async fn copy(value: String) -> Result<(), String> {
    let mut bridge = document::eval(
        "const value = await dioxus.recv(); try { await window.aioPlugin.copy(value); dioxus.send(null); } catch (_) { dioxus.send('复制未获授权'); }",
    );
    bridge.send(value).map_err(|e| e.to_string())?;
    let error: Option<String> = bridge.recv().await.map_err(|e| e.to_string())?;
    error.map_or(Ok(()), Err)
}
