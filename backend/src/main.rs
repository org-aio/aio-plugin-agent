use anyhow::{Context, Result, ensure};
use az_agent_server::{
    configuration::RuntimeConfig,
    transport::{Ingress, router},
};

#[tokio::main]
async fn main() -> Result<()> {
    let config = RuntimeConfig::from_env()?;
    let token = std::env::var("AIO_AGENT_INGRESS_TOKEN").context("缺少宿主入口票据")?;
    ensure!(token.len() >= 32, "入口票据至少 32 字节");
    let service = az_agent_server::service(config).await?;
    let port = std::env::var("AIO_PLUGIN_PORT")
        .unwrap_or_else(|_| "4193".into())
        .parse::<u16>()?;
    let bind = std::env::var("AIO_AGENT_BIND")
        .unwrap_or_else(|_| "127.0.0.1".into())
        .parse::<std::net::IpAddr>()?;
    let listener = tokio::net::TcpListener::bind((bind, port)).await?;
    println!("Agent backend listening on {bind}:{port}");
    let stopping = service.clone();
    axum::serve(listener, router(service, Ingress { token }))
        .with_graceful_shutdown(async move {
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
            stopping.shutdown().await;
        })
        .await?;
    Ok(())
}
