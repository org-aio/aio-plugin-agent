use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{collections::BTreeSet, time::Duration};
use url::Url;

#[derive(Clone)]
pub struct RuntimeConfig {
    pub database_url: String,
    pub encryption_key: [u8; 32],
    pub allowed_endpoints: BTreeSet<String>,
    pub generation_timeout: Duration,
    pub allow_loopback: bool,
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self> {
        let key = STANDARD
            .decode(std::env::var("AIO_AGENT_MASTER_KEY").context("缺少 AIO_AGENT_MASTER_KEY")?)?;
        let config = Self {
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
