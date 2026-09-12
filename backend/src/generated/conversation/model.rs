use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
pub use az_agent_model::*;

#[derive(Clone, Debug)]
pub struct Scope {
    pub tenant: String,
    pub user: String,
    pub context_id: Option<String>,
}

pub(super) struct ModelConnection {
    pub endpoint: String,
    pub model: String,
    pub secret: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRequest {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub body: serde_json::Value,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MemorySearch {
    pub query: String,
}

pub struct ServiceError(pub StatusCode, pub String);
pub type ServiceResult<T> = Result<T, ServiceError>;

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        (self.0, Json(Failure { error: self.1 })).into_response()
    }
}
impl From<anyhow::Error> for ServiceError {
    fn from(_: anyhow::Error) -> Self {
        Self(StatusCode::SERVICE_UNAVAILABLE, "服务暂时不可用".into())
    }
}
impl From<sqlx::Error> for ServiceError {
    fn from(_: sqlx::Error) -> Self {
        Self(StatusCode::SERVICE_UNAVAILABLE, "数据库操作失败".into())
    }
}
pub fn bad(message: &str) -> ServiceError {
    ServiceError(StatusCode::BAD_REQUEST, message.into())
}
pub fn missing() -> ServiceError {
    ServiceError(StatusCode::NOT_FOUND, "记录不存在".into())
}
pub fn conflict(message: &str) -> ServiceError {
    ServiceError(StatusCode::CONFLICT, message.into())
}
