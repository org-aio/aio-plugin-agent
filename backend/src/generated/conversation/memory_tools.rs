use super::{
    memory,
    model::{MemoryCitation, MemorySearch, Scope},
    service_impl::Core,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

pub(super) struct MemoryTools {
    pub core: Arc<Core>,
    pub scope: Scope,
    pub space: String,
    pub assistant: Uuid,
    pub source: Option<String>,
}

#[async_trait::async_trait]
impl crate::runtime::Tool for MemoryTools {
    fn definition(&self) -> Value {
        json!({"type":"function","function":{"name":"memory_search","description":"检索当前空间的相关记忆；闲聊无需检索。","parameters":{"type":"object","properties":{"query":{"type":"string","maxLength":180}},"required":["query"],"additionalProperties":false}}})
    }
    async fn invoke(&self, arguments: Value) -> Result<Value> {
        let search: MemorySearch = serde_json::from_value(arguments)?;
        ensure!(
            !search.query.trim().is_empty() && search.query.chars().count() <= 180,
            "检索条件无效"
        );
        let graph = memory::invoke(
            &self.core,
            &self.scope,
            "POST",
            &format!("/recall?spaceId={}", self.space),
            json!({"query":search.query,"limit":4,"excludeIds":self.source.iter().collect::<Vec<_>>()}),
            false,
        )
        .await?;
        let nodes = graph["nodes"].as_array().context("检索结果无效")?;
        let matched: Vec<String> = nodes
            .iter()
            .filter_map(|node| node["id"].as_str().map(str::to_owned))
            .collect();
        if matched.is_empty() {
            return Ok(json!({"context":"没有找到相关资料","citations":[]}));
        }
        let context = memory::invoke(
            &self.core,
            &self.scope,
            "POST",
            &format!("/context?spaceId={}", self.space),
            json!({"nodeIds":matched,"depth":1,"maxCharacters":6000}),
            false,
        )
        .await?;
        let ids: Vec<String> = serde_json::from_value(context["nodeIds"].clone())?;
        let references: Vec<MemoryCitation> = ids
            .iter()
            .map(|id| MemoryCitation {
                id: id.clone(),
                title: nodes
                    .iter()
                    .find(|node| node["id"] == *id)
                    .and_then(|node| node["title"].as_str())
                    .unwrap_or("关联资料")
                    .into(),
            })
            .collect();
        let mut tx = self.core.pool.begin().await?;
        let row = sqlx::query("SELECT citations,matched_node_ids,activated_node_ids FROM agent_messages WHERE id=$1 FOR UPDATE")
            .bind(self.assistant).fetch_one(&mut *tx).await?;
        let mut citations: Vec<MemoryCitation> = serde_json::from_value(row.get("citations"))?;
        for citation in &references {
            if !citations.iter().any(|value| value.id == citation.id) {
                citations.push(citation.clone());
            }
        }
        let merge = |field: &str, values: &[String]| -> Result<Vec<String>> {
            let mut combined: Vec<String> = serde_json::from_value(row.get(field))?;
            for id in values {
                if !combined.contains(id) && combined.len() < 24 {
                    combined.push(id.clone());
                }
            }
            Ok(combined)
        };
        sqlx::query("UPDATE agent_messages SET citations=$2,matched_node_ids=$3,activated_node_ids=$4 WHERE id=$1")
            .bind(self.assistant).bind(serde_json::to_value(citations)?)
            .bind(serde_json::to_value(merge("matched_node_ids", &matched)?)?)
            .bind(serde_json::to_value(merge("activated_node_ids", &ids)?)?)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"context":context["markdown"],"citations":references}))
    }
}
