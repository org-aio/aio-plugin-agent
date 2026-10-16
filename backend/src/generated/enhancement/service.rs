use std::any::Any;

use anyhow::Result;
use uuid::Uuid;

use super::model::{SourceRequest, SourceResponse};
use crate::generated::conversation::model::Scope;

/// 使用部署方选定的模型生成源码；调用方绑定身份，不接受脚本传入的凭据或用户。
#[async_trait::async_trait]
pub trait EnhancementService: Any + Send + Sync {
    async fn enhancement_source(
        &self,
        scope: &Scope,
        provider: Uuid,
        request: SourceRequest,
    ) -> Result<SourceResponse>;
}
