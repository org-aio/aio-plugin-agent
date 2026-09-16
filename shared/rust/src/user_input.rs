use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// 问题由协调器发给当前会话用户；选项值不能充当权限凭据。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputQuestion {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub options: Vec<InputOption>,
    #[serde(default = "allow_text")]
    pub allow_text: bool,
}
fn allow_text() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserInputRequest {
    pub id: Uuid,
    pub assistant_id: Uuid,
    pub questions: Vec<InputQuestion>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputAnswer {
    pub request_id: Uuid,
    pub answers: BTreeMap<String, String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceSelection {
    pub worker_id: Option<Uuid>,
}
