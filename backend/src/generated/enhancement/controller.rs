use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    service::RequestContext,
    transport::{
        StreamableHttpService,
        streamable_http_server::{StreamableHttpServerConfig, session::local::LocalSessionManager},
    },
};
use serde_json::json;
use subtle::ConstantTimeEq;

use super::{
    configuration::Binding,
    model::{MAX_BYTES, SourceRequest, SourceResponse},
    service::EnhancementService,
};
use crate::generated::conversation::model::Scope;

const TOOL: &str = "enhancement_source";

#[derive(Clone)]
struct ModelServer {
    service: Arc<dyn EnhancementService>,
    binding: Binding,
}

/// 可选 MCP 入口；部署凭据固定关联一个模型所有者，绝不转发客户端提供的身份。
pub fn router(service: Arc<dyn EnhancementService>, binding: Binding) -> Result<Router> {
    let origin = binding.validate()?;
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_max_request_body_bytes(MAX_BYTES * 2)
        .with_allowed_hosts([origin.host_str().context("模型 MCP 缺少主机")?])
        .with_allowed_origins([origin.origin().ascii_serialization()])
        .enforce_origin_validation();
    let state = ModelServer { service, binding };
    let server = state.clone();
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    Ok(Router::new()
        .route_service("/mcp/enhancements", service)
        .route_layer(axum::middleware::from_fn_with_state(state, authenticate)))
}

async fn authenticate(State(state): State<ModelServer>, request: Request, next: Next) -> Response {
    let token = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    if token
        .as_bytes()
        .ct_eq(state.binding.token.as_bytes())
        .unwrap_u8()
        != 1
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

impl ServerHandler for ModelServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("只生成 Rhai/TypeScript 源码。调用会消耗部署方绑定模型的额度；不执行源码，不验证候选，也不激活版本。")
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if request.is_some_and(|value| value.cursor.is_some()) {
            return Err(ErrorData::invalid_params(
                "模型能力目录不使用分页游标",
                None,
            ));
        }
        let input = schemars::schema_for!(SourceRequest)
            .as_object()
            .cloned()
            .ok_or_else(|| ErrorData::internal_error("模型输入契约无效", None))?;
        let output = schemars::schema_for!(SourceResponse)
            .as_object()
            .cloned()
            .ok_or_else(|| ErrorData::internal_error("模型输出契约无效", None))?;
        let mut tool = Tool::new(TOOL, "按既定契约生成增强源码，消耗模型额度", input)
            .with_annotations(ToolAnnotations::new().read_only(false).idempotent(false));
        tool.output_schema = Some(Arc::new(output));
        Ok(ListToolsResult::with_all_items(vec![tool]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if request.name != TOOL {
            return Err(ErrorData::invalid_params("未知模型工具", None));
        }
        let request: SourceRequest =
            serde_json::from_value(json!(request.arguments.unwrap_or_default()))
                .map_err(|_| ErrorData::invalid_params("增强模型输入格式无效", None))?;
        let scope = Scope {
            tenant: self.binding.tenant_id.clone(),
            user: self.binding.user_id.clone(),
            context_id: None,
        };
        let generate = self
            .service
            .enhancement_source(&scope, self.binding.provider_id, request);
        let result = tokio::select! {
            result = generate => result,
            _ = context.ct.cancelled() => return Err(ErrorData::internal_error("源码生成已取消", None)),
        };
        match result {
            Ok(response) => Ok(CallToolResult::structured(json!(response)).into()),
            Err(error) => {
                eprintln!("增强源码生成失败：{error:#}");
                Ok(CallToolResult::structured_error(
                    json!({"error":"模型未授权、配额不足、生成失败或返回格式无效"}),
                )
                .into())
            }
        }
    }
}
