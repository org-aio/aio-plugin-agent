use serde::{Deserialize, Serialize};

/// Skill 文件按相对路径同步，哈希同时包含内容与可执行标记。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFile {
    pub path: String,
    pub hash: Option<String>,
    pub executable: bool,
    pub size: i64,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLibrary {
    pub files: Vec<SkillFile>,
    pub devices: Vec<SkillDevice>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDevice {
    pub id: String,
    pub updated_at: i64,
    pub report: serde_json::Value,
    pub resolutions: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillContent {
    pub file: SkillFile,
    pub content: Option<String>,
}
