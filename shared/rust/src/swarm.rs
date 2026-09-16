use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// 会话里的设备执行回执；刷新页面后按持久化任务 ID 继续查询。
#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SwarmTask {
    pub id: Uuid,
    pub assistant_id: Uuid,
    pub worker_id: Uuid,
    pub device: String,
    pub label: String,
    pub state: String,
    pub result: Option<Value>,
    pub error: Option<String>,
}
