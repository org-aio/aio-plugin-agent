use super::model::Request;
use crate::generated::conversation::{
    model::{Scope, ServiceResult},
    service::AgentService,
};
use axum::{Extension, Json, extract::State};
use serde_json::Value;
use std::sync::Arc;

pub async fn page(
    State(service): State<Arc<dyn AgentService>>,
    Extension(scope): Extension<Scope>,
    Json(request): Json<Request>,
) -> ServiceResult<Json<Value>> {
    Ok(Json(service.skill_request(&scope, request, false).await?))
}
pub async fn worker(
    State(service): State<Arc<dyn AgentService>>,
    Extension(scope): Extension<Scope>,
    Json(request): Json<Request>,
) -> ServiceResult<Json<Value>> {
    Ok(Json(service.skill_request(&scope, request, true).await?))
}
