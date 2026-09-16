use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchSettings {
    pub enabled: bool,
    pub has_secret: bool,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchSettingsDraft {
    pub enabled: bool,
    /// 未传时保留，空字符串清除；读取接口只返回 hasSecret。
    pub secret: Option<String>,
}
