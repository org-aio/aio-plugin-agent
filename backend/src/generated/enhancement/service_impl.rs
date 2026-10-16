use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use az_agent_engine::{Delta, RunState};
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{
    model::{MAX_BYTES, SourceRequest, SourceResponse},
    service::EnhancementService,
    util,
};
use crate::generated::conversation::{
    metering, model::Scope, model_access, service_impl::AgentServiceImpl,
};

#[async_trait::async_trait]
impl EnhancementService for AgentServiceImpl {
    async fn enhancement_source(
        &self,
        scope: &Scope,
        provider: Uuid,
        request: SourceRequest,
    ) -> Result<SourceResponse> {
        let messages = util::messages(&request)?;
        let core = &self.core;
        ensure!(!core.shutdown.is_cancelled(), "Agent 正在停止");
        let _permit = core.quota.try_acquire().context("模型执行并发已满")?;
        let operation = async {
            let connection = model_access::personal(core, scope, provider).await?;
            let mut state = RunState::new(messages);
            state.rounds_left = 1;
            let (sender, mut receiver) = mpsc::channel(8);
            let generate = crate::runtime::continue_run(
                &core.client,
                core.config.gateway.as_ref(),
                &connection.endpoint,
                &connection.model,
                connection.secret.as_deref(),
                state,
                vec![],
                sender,
            );
            let collect = async {
                let mut text = String::new();
                let mut tokens = None;
                while let Some(delta) = receiver.recv().await {
                    match delta {
                        Delta::Text(part) => {
                            ensure!(
                                text.len().saturating_add(part.len()) <= MAX_BYTES,
                                "模型输出超过配额"
                            );
                            text.push_str(&part);
                        }
                        Delta::Tokens(value) => tokens = Some(value),
                        Delta::Waiting { .. } => bail!("源码生成不能请求工具或用户交互"),
                    }
                }
                Ok::<_, anyhow::Error>((text, tokens))
            };
            // 同一 Future 内并行消费有界通道；取消或超额会同时释放模型连接和配额。
            let (_, (text, tokens)) = tokio::try_join!(generate, collect)?;
            metering::record_tokens(core, scope, Uuid::new_v4(), tokens).await;
            let current = model_access::personal(core, scope, provider).await?;
            ensure!(current == connection, "生成期间模型授权或配置已变化");
            util::response(&text)
        };
        let timeout = core.config.generation_timeout.min(Duration::from_secs(25));
        tokio::select! {
            _ = core.shutdown.cancelled() => bail!("Agent 正在停止"),
            result = tokio::time::timeout(timeout, operation) => result.context("源码生成超时")?,
        }
    }
}
