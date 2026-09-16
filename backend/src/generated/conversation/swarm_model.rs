use serde::Deserialize;
use serde_json::Value;

/// 一组独立工作交给同一设备，工作区逻辑名称由本机授权配置解析。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Dispatch {
    pub groups: Vec<Group>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Group {
    pub device: Option<String>,
    pub label: String,
    pub input: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskIds {
    pub task_ids: Vec<uuid::Uuid>,
}
