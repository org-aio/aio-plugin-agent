use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{collections::BTreeSet, time::Duration};
use url::Url;

#[derive(Clone)]
pub struct RuntimeConfig {
    pub engine: crate::runtime::EngineConfig,
    pub gateway: Option<Gateway>,
    pub database_url: String,
    pub encryption_key: [u8; 32],
    pub allowed_endpoints: BTreeSet<String>,
    pub generation_timeout: Duration,
    pub allow_loopback: bool,
    pub memory: Option<MemoryConnection>,
}

#[derive(Clone)]
pub struct Gateway {
    pub socket: std::path::PathBuf,
    pub token: String,
}

#[derive(Clone)]
pub struct MemoryConnection {
    pub endpoint: Url,
    pub token: String,
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self> {
        let key = STANDARD
            .decode(std::env::var("AIO_AGENT_MASTER_KEY").context("缺少 AIO_AGENT_MASTER_KEY")?)?;
        let config = Self {
            gateway: None,
            engine: crate::runtime::EngineConfig::from_env()?,
            database_url: std::env::var("AIO_AGENT_DATABASE_URL")
                .context("缺少插件专属数据库连接")?,
            encryption_key: key
                .try_into()
                .map_err(|_| anyhow::anyhow!("主密钥必须是 32 字节 Base64"))?,
            allowed_endpoints: std::env::var("AIO_AGENT_ENDPOINTS")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().trim_end_matches('/').to_owned())
                .collect(),
            generation_timeout: Duration::from_secs(120),
            allow_loopback: std::env::var("AIO_AGENT_ALLOW_LOOPBACK").as_deref() == Ok("1"),
            memory: match std::env::var("AIO_AGENT_MEMORY_URL") {
                Ok(value) => {
                    let endpoint = Url::parse(&value).context("记忆服务绑定无效")?;
                    let local = endpoint
                        .host_str()
                        .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]"));
                    ensure!(
                        (endpoint.scheme() == "https"
                            || (local
                                && std::env::var("AIO_AGENT_ALLOW_LOOPBACK").as_deref()
                                    == Ok("1")))
                            && endpoint.username().is_empty()
                            && endpoint.password().is_none()
                            && endpoint.query().is_none()
                            && endpoint.fragment().is_none(),
                        "记忆服务绑定必须为宿主授权的 HTTPS 地址"
                    );
                    let token = std::env::var("AIO_AGENT_MEMORY_TOKEN")
                        .context("记忆服务调用凭据未配置")?;
                    ensure!(token.len() >= 32, "记忆服务调用凭据无效");
                    Some(MemoryConnection { endpoint, token })
                }
                Err(_) => None,
            },
        };
        for endpoint in &config.allowed_endpoints {
            config.endpoint(endpoint)?;
        }
        Ok(config)
    }

    pub fn endpoint(&self, value: &str) -> Result<Url> {
        let value = value.trim_end_matches('/');
        ensure!(
            self.allowed_endpoints.contains(value),
            "模型地址未被宿主授权"
        );
        if self.gateway.is_some() {
            return az_plugin_contract::process::model_endpoint(value).map_err(anyhow::Error::msg);
        }
        let url = Url::parse(value).context("模型地址无效")?;
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "模型地址不能包含凭据、查询或锚点"
        );
        let local = url
            .host_str()
            .is_some_and(|s| s == "127.0.0.1" || s == "[::1]" || s == "localhost");
        ensure!(
            url.scheme() == "https" || (self.allow_loopback && local && url.scheme() == "http"),
            "模型地址必须使用 HTTPS，开发回环地址需显式授权"
        );
        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_http_requires_gateway_and_an_exact_host_grant() {
        let endpoint = "http://192.168.31.252:18080/v1";
        let mut config = RuntimeConfig {
            engine: crate::runtime::EngineConfig {
                node: "node".into(),
                directory: "runtime".into(),
            },
            gateway: None,
            database_url: String::new(),
            encryption_key: [0; 32],
            allowed_endpoints: [endpoint.to_owned()].into(),
            generation_timeout: Duration::from_secs(120),
            allow_loopback: false,
            memory: None,
        };
        assert!(config.endpoint(endpoint).is_err());
        config.gateway = Some(Gateway {
            socket: "/broker/gateway.sock".into(),
            token: String::new(),
        });
        assert!(config.endpoint(endpoint).is_ok());
        assert!(config.endpoint("http://192.168.31.253:18080/v1").is_err());
        config
            .allowed_endpoints
            .insert("http://169.254.169.254/latest".into());
        assert!(config.endpoint("http://169.254.169.254/latest").is_err());
    }
}
