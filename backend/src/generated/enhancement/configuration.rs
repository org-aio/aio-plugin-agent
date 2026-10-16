use std::{io::Read, path::Path};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use uuid::Uuid;

use crate::transport::Ingress;

/// 独立模型服务的部署绑定；密钥仅授权生成源码，不授予 Agent 管理入口。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub tenant_id: String,
    pub user_id: String,
    pub provider_id: Uuid,
    pub token: String,
    pub public_origin: String,
}

impl Binding {
    /// 从部署方私有文件加载；不从脚本参数或 HTTP 身份头选择模型所有者。
    pub fn load(path: &Path, ingress: &Ingress) -> Result<Self> {
        let file = std::fs::File::open(path).context("打开增强模型绑定失败")?;
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "增强模型绑定必须是普通文件");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                metadata.permissions().mode() & 0o077 == 0,
                "增强模型绑定必须仅允许所有者读取"
            );
        }
        let mut bytes = Vec::new();
        file.take(16385).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16384, "增强模型绑定文件过大");
        let binding: Self = serde_json::from_slice(&bytes).context("增强模型绑定格式无效")?;
        binding.validate()?;
        ensure!(
            binding.token != ingress.token,
            "模型生成与 Agent 管理必须使用不同票据"
        );
        ensure!(
            ingress
                .tenant
                .as_ref()
                .is_none_or(|tenant| tenant == &binding.tenant_id),
            "模型绑定租户与宿主不一致"
        );
        Ok(binding)
    }

    pub(super) fn validate(&self) -> Result<url::Url> {
        ensure!(
            [&self.tenant_id, &self.user_id].iter().all(|id| {
                !id.trim().is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
            }) && !self.provider_id.is_nil(),
            "模型绑定身份无效"
        );
        ensure!(
            (32..=4096).contains(&self.token.len())
                && self
                    .token
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')),
            "模型生成票据必须是至少 32 字节的随机 URL 安全文本"
        );
        let origin = url::Url::parse(&self.public_origin).context("模型 MCP 公开地址无效")?;
        let host = origin.host_str().context("模型 MCP 公开地址缺少主机")?;
        let private = host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| match ip {
                std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
                std::net::IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
            });
        ensure!(
            (origin.scheme() == "https" || (origin.scheme() == "http" && private))
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none(),
            "模型 MCP 地址必须是 HTTPS 或固定本机/私网 IP 的 origin"
        );
        Ok(origin)
    }
}
