use anyhow::{Context, Result, ensure};
use az_agent_server::{
    configuration::RuntimeConfig,
    transport::{Ingress, router},
};

#[tokio::main]
async fn main() -> Result<()> {
    let (config, ingress) = match az_agent_server::hosting::load()? {
        Some(hosted) => hosted,
        None => {
            let token = std::env::var("AIO_AGENT_INGRESS_TOKEN").context("缺少宿主入口票据")?;
            ensure!(token.len() >= 32, "入口票据至少 32 字节");
            (
                RuntimeConfig::from_env()?,
                Ingress {
                    token,
                    tenant: None,
                },
            )
        }
    };
    let service = az_agent_server::service(config).await?;
    let app = router(service.clone(), ingress);
    if let Some(path) = std::env::var_os("AIO_PLUGIN_SOCKET") {
        use std::os::unix::fs::PermissionsExt;
        if tokio::fs::try_exists(&path).await? {
            tokio::fs::remove_file(&path).await?;
        }
        let listener = tokio::net::UnixListener::bind(&path)?;
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown(service))
            .await?;
        return Ok(());
    }
    let port = std::env::var("AIO_PLUGIN_PORT")
        .unwrap_or_else(|_| "4193".into())
        .parse::<u16>()?;
    let bind = std::env::var("AIO_AGENT_BIND")
        .unwrap_or_else(|_| "127.0.0.1".into())
        .parse::<std::net::IpAddr>()?;
    let listener = tokio::net::TcpListener::bind((bind, port)).await?;
    println!("Agent backend listening on {bind}:{port}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown(service))
        .await?;
    Ok(())
}

async fn shutdown(
    service: std::sync::Arc<dyn az_agent_server::generated::conversation::service::AgentService>,
) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _=ctrl_c=>{},_=terminate=>{} }
    service.shutdown().await;
}
