use super::model::Request;
use crate::generated::conversation::model::{Scope, ServiceResult};
use serde_json::Value;

/// 页面与已配对设备共用一份用户技能库；设备身份只从宿主上下文读取。
#[async_trait::async_trait]
pub trait SkillService: Send + Sync {
    async fn skill_request(
        &self,
        scope: &Scope,
        request: Request,
        worker: bool,
    ) -> ServiceResult<Value>;
}
