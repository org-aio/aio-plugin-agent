use anyhow::{Context, Result, ensure};
use az_plugin_contract::process::Configuration;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::path::PathBuf;

use crate::{
    configuration::{Gateway, MemoryConnection, RuntimeConfig},
    transport::Ingress,
};

pub const MEMORY_SOURCE: &str = "https://github.com/zjarlin/aio-plugin-agent-memory.git";

pub fn load() -> Result<Option<(RuntimeConfig, Ingress)>> {
    let Some(path) = std::env::var_os("AIO_PLUGIN_CONFIG") else {
        return Ok(None);
    };
    let metadata = std::fs::metadata(&path)?;
    ensure!(
        metadata.is_file() && metadata.len() <= 65536,
        "宿主配置无效"
    );
    let host: Configuration =
        serde_json::from_slice(&std::fs::read(path)?).context("宿主配置协议无效")?;
    ensure!(
        host.abi_version == 2
            && !host.tenant_id.is_empty()
            && host.ingress_token.len() >= 32
            && PathBuf::from(&host.broker_socket).is_absolute(),
        "宿主绑定无效"
    );
    let encryption_key = STANDARD
        .decode(host.encryption_key.context("宿主未授权加密")?)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("宿主密钥格式无效"))?;
    let memory = host
        .services
        .iter()
        .any(|git| git == MEMORY_SOURCE)
        .then(|| -> Result<MemoryConnection> {
            Ok(MemoryConnection {
                endpoint: "http://localhost/invoke".parse()?,
                token: host.ingress_token.clone(),
            })
        })
        .transpose()?;
    let config = RuntimeConfig {
        gateway: Some(Gateway {
            socket: host.broker_socket.into(),
            token: host.ingress_token.clone(),
            worker_capabilities: host.worker_capabilities,
        }),
        database_url: host.database_url.context("宿主未授权数据库")?,
        encryption_key,
        allowed_endpoints: host.endpoints.into_iter().collect(),
        generation_timeout: std::time::Duration::from_secs(120),
        allow_loopback: false,
        memory,
    };
    for endpoint in &config.allowed_endpoints {
        config.endpoint(endpoint)?;
    }
    Ok(Some((
        config,
        Ingress {
            token: host.ingress_token,
            tenant: Some(host.tenant_id),
        },
    )))
}

pub async fn describe() -> axum::Json<serde_json::Value> {
    axum::Json(
        serde_json::json!({"label":"智能体","pages":[{"id":"chat","label":"对话","entry":"index.html","scene":["workspace","工作空间"],"menu_path":["智能体"],"permission":null,"surface":"workspace"},{"id":"skills","label":"Skill 管理","entry":"skills.html","scene":["workspace","工作空间"],"menu_path":["智能体"],"permission":null,"surface":"workspace"},{"id":"settings","label":"智能体设置","entry":"settings.html","scene":null,"menu_path":[],"permission":null,"surface":"fullscreen"}]}),
    )
}
