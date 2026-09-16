use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraph {
    pub nodes: Vec<MemoryNode>,
    pub edges: Vec<MemoryEdge>,
    pub total: i64,
    pub truncated: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNode {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub content: String,
    pub url: String,
    pub tags: Vec<String>,
    pub version: i64,
    pub updated_at: i64,
    pub aliases: Vec<String>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MemoryEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub relation: String,
    pub evidence: String,
}
