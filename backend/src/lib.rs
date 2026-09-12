pub mod configuration;
pub mod generated;
pub mod runtime;
pub mod transport;

use anyhow::Result;
use dill::CatalogBuilder;
use generated::conversation::{service::AgentService, service_impl::AgentServiceImpl};
use std::sync::Arc;

pub async fn service(config: configuration::RuntimeConfig) -> Result<Arc<dyn AgentService>> {
    let implementation = AgentServiceImpl::connect(config).await?;
    let mut builder = CatalogBuilder::new();
    builder
        .add_value(implementation)
        .bind::<dyn AgentService, AgentServiceImpl>();
    Ok(builder.build().get_one::<dyn AgentService>()?)
}
