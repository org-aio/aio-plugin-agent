use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Clone)]
pub struct EngineConfig {
    pub node: String,
    pub directory: PathBuf,
}

impl EngineConfig {
    pub fn from_env() -> Result<Self> {
        let directory = PathBuf::from(
            std::env::var("AIO_AGENT_RUNTIME_DIR").unwrap_or_else(|_| "runtime".into()),
        )
        .canonicalize()
        .context("Agent 运行时目录不存在")?;
        anyhow::ensure!(
            directory.join("main.mjs").is_file(),
            "Agent 运行时入口不存在"
        );
        Ok(Self {
            node: std::env::var("AIO_AGENT_NODE").unwrap_or_else(|_| "node".into()),
            directory,
        })
    }
}

pub enum Delta {
    Text(String),
    Tokens(i64),
}

#[async_trait::async_trait]
pub trait ToolHandler: Send + Sync {
    async fn invoke(&self, name: &str, arguments: serde_json::Value) -> Result<serde_json::Value>;
}
