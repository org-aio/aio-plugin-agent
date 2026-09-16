use crate::generated::conversation::{controller, model::Scope, service::AgentService};
use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::sync::Arc;
use subtle::ConstantTimeEq;

#[derive(Clone)]
pub struct Ingress {
    pub token: String,
    pub tenant: Option<String>,
}

async fn authenticate(
    State(ingress): State<Ingress>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get("x-aio-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if token.as_bytes().ct_eq(ingress.token.as_bytes()).unwrap_u8() != 1 {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let scope = {
        let read = |name: &str| {
            request
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .filter(|v| !v.is_empty() && v.len() <= 128)
                .map(str::to_owned)
        };
        let (Some(tenant), Some(user)) = (read("x-aio-tenant-id"), read("x-aio-user-id")) else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if ingress
            .tenant
            .as_ref()
            .is_some_and(|bound| bound != &tenant)
        {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Scope {
            tenant,
            user,
            context_id: read("x-aio-context"),
        }
    };
    request.extensions_mut().insert(scope);
    next.run(request).await
}

pub fn router(service: Arc<dyn AgentService>, ingress: Ingress) -> Router {
    let application = Router::new()
        .route("/skills", post(crate::generated::skills::controller::page))
        .route(
            "/worker/skills.sync",
            post(crate::generated::skills::controller::worker),
        )
        .route("/settings", get(controller::settings))
        .route(
            "/tools/web-search",
            axum::routing::put(controller::save_search),
        )
        .route("/memory", post(controller::memory_request))
        .route("/providers", post(controller::add_provider))
        .route("/providers/models", post(controller::models))
        .route(
            "/providers/{id}",
            axum::routing::put(controller::edit_provider).delete(controller::delete_provider),
        )
        .route(
            "/conversations",
            get(controller::list).post(controller::create),
        )
        .route(
            "/conversations/{id}",
            get(controller::thread).delete(controller::delete),
        )
        .route("/devices", get(controller::devices))
        .route(
            "/conversations/{id}/device",
            axum::routing::put(controller::select_device),
        )
        .route("/conversations/{id}/input", post(controller::answer_input))
        .route("/conversations/{id}/messages", post(controller::send))
        .route(
            "/conversations/{id}/model",
            axum::routing::put(controller::select_model),
        )
        .route("/conversations/{id}/cancel", post(controller::cancel))
        .layer(DefaultBodyLimit::max(3 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(ingress, authenticate))
        .with_state(service);
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/aio/describe", get(crate::hosting::describe))
        .merge(application)
}
