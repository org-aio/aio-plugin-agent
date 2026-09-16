use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;

pub enum Delta {
    Text(String),
    Tokens(i64),
}

/// 工具名称属于模型线协议；实例由调用方注入，不用名称注册应用插件。
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> Value;
    async fn invoke(&self, arguments: Value) -> Result<Value>;
}

#[derive(Default)]
pub(crate) struct Completion {
    pub text: String,
    pub calls: std::collections::BTreeMap<usize, ToolCall>,
    pub tokens: i64,
    pub finished: bool,
}

#[derive(Default, serde::Serialize)]
pub(crate) struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: Function,
}

#[derive(Default, serde::Serialize)]
pub(crate) struct Function {
    pub name: String,
    pub arguments: String,
}

#[derive(Deserialize)]
pub(crate) struct Chunk {
    #[serde(default)]
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
    pub error: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct Usage {
    pub total_tokens: i64,
}

#[derive(Deserialize)]
pub(crate) struct Choice {
    pub delta: Change,
    pub finish_reason: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct Change {
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<CallChange>,
}

#[derive(Deserialize)]
pub(crate) struct CallChange {
    pub index: usize,
    pub id: Option<String>,
    pub function: Option<FunctionChange>,
}

#[derive(Deserialize)]
pub(crate) struct FunctionChange {
    pub name: Option<String>,
    pub arguments: Option<String>,
}
