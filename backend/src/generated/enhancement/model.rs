use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub(super) const MAX_BYTES: usize = 64 * 1024;
pub(super) const MAX_SOURCE_BYTES: usize = 16 * 1024;

/// 与双语言自举策略共用的模型输入；目标契约归运行时所有，模型不负责修改它。
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceRequest {
    pub requirement: String,
    pub target: Map<String, Value>,
    pub available: Vec<Map<String, Value>>,
}

/// 模型唯一允许返回的字段；身份、依赖、样例、版本和激活代数均不由模型指定。
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceResponse {
    pub source: String,
}
