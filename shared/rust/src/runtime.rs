use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const RUNTIME_PROTOCOL_VERSION: u32 = 1;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeCommand<'a> {
    Start {
        version: u32,
        model: &'a str,
        messages: Vec<Value>,
        tools: Vec<&'a str>,
    },
    Response {
        id: u32,
        status: u16,
    },
    Chunk {
        id: u32,
        data: String,
    },
    End {
        id: u32,
    },
    ToolResult {
        id: u32,
        value: Value,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeEvent {
    Fetch {
        id: u32,
        url: String,
        body: Value,
    },
    Tool {
        id: u32,
        name: String,
        arguments: Value,
    },
    Text {
        text: String,
    },
    Usage {
        tokens: i64,
    },
    Done,
    Error,
}
