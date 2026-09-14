use super::{model::*, service::AgentService};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use std::sync::Arc;
use uuid::Uuid;

type Service = State<Arc<dyn AgentService>>;
type Identity = Extension<Scope>;
pub async fn models(
    State(service): Service,
    Extension(scope): Identity,
    Json(request): Json<ModelListRequest>,
) -> ServiceResult<Json<Vec<String>>> {
    Ok(Json(service.models(&scope, request).await?))
}
pub async fn select_model(
    State(service): Service,
    Extension(scope): Identity,
    Path(id): Path<Uuid>,
    Json(selection): Json<ModelSelection>,
) -> ServiceResult<Json<Conversation>> {
    Ok(Json(service.select_model(&scope, id, selection).await?))
}
pub async fn settings(
    State(service): Service,
    Extension(scope): Identity,
) -> ServiceResult<Json<Settings>> {
    Ok(Json(service.settings(&scope).await?))
}
pub async fn add_provider(
    State(service): Service,
    Extension(scope): Identity,
    Json(draft): Json<ProviderDraft>,
) -> ServiceResult<Json<Provider>> {
    Ok(Json(service.save_provider(&scope, None, draft).await?))
}
pub async fn edit_provider(
    State(service): Service,
    Extension(scope): Identity,
    Path(id): Path<Uuid>,
    Json(draft): Json<ProviderDraft>,
) -> ServiceResult<Json<Provider>> {
    Ok(Json(service.save_provider(&scope, Some(id), draft).await?))
}
pub async fn delete_provider(
    State(service): Service,
    Extension(scope): Identity,
    Path(id): Path<Uuid>,
) -> ServiceResult<StatusCode> {
    service.delete_provider(&scope, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn list(
    State(service): Service,
    Extension(scope): Identity,
) -> ServiceResult<Json<Vec<Conversation>>> {
    Ok(Json(service.conversations(&scope).await?))
}
pub async fn create(
    State(service): Service,
    Extension(scope): Identity,
    Json(draft): Json<ConversationDraft>,
) -> ServiceResult<Json<Conversation>> {
    Ok(Json(service.create(&scope, draft).await?))
}
pub async fn thread(
    State(service): Service,
    Extension(scope): Identity,
    Path(id): Path<Uuid>,
) -> ServiceResult<Json<Thread>> {
    Ok(Json(service.thread(&scope, id).await?))
}
pub async fn delete(
    State(service): Service,
    Extension(scope): Identity,
    Path(id): Path<Uuid>,
) -> ServiceResult<StatusCode> {
    service.delete(&scope, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn send(
    State(service): Service,
    Extension(scope): Identity,
    Path(id): Path<Uuid>,
    Json(prompt): Json<Prompt>,
) -> ServiceResult<Json<Thread>> {
    Ok(Json(service.send(&scope, id, prompt).await?))
}
pub async fn cancel(
    State(service): Service,
    Extension(scope): Identity,
    Path(id): Path<Uuid>,
) -> ServiceResult<Json<Thread>> {
    Ok(Json(service.cancel(&scope, id).await?))
}

pub async fn memory_request(
    State(service): Service,
    Extension(scope): Identity,
    Json(request): Json<MemoryRequest>,
) -> ServiceResult<impl axum::response::IntoResponse> {
    let result = service
        .memory_request(&scope, &request.method, &request.path, request.body)
        .await?;
    Ok((
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(result),
    ))
}
