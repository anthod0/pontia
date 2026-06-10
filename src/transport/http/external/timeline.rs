use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::application::{
    AgentBinding, AgentBindingResolveRequest, AgentBindingResolver, AgentBindingService, AppState,
    ExternalQueryService, PiAgentBindingResolver, PiJsonlParser, RawTranscriptParser,
    ResolvedAgentBinding, TimelineItemDetailRequest, TimelinePageRequest,
};

use super::common::{ApiResponse, ExternalApiError, authenticate, ensure_session_exists, ok};

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct TimelineDetailQuery {
    #[serde(rename = "ref")]
    content_ref: String,
}

pub async fn get_session_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let query_service = ExternalQueryService::new(state.db.clone());
    ensure_session_exists(&query_service, &session_id).await?;

    let binding = AgentBindingService::new(state.db.clone())
        .primary_binding_for_session(&session_id)
        .await?
        .ok_or_else(|| {
            timeline_error(
                "not_ready",
                format!("session {session_id} has no agent binding"),
            )
        })?;

    let parser = PiJsonlParser::new();
    let page = parser
        .timeline_page(TimelinePageRequest {
            session_id,
            source: resolve_binding_source(&binding)?,
            cursor: query.cursor,
            limit: query.limit.unwrap_or(50),
        })
        .map_err(timeline_error_from_error)?;

    Ok(ok(json!({
        "session_id": page.session_id,
        "binding_id": page.binding_id,
        "items": page.items,
        "next_cursor": page.next_cursor,
        "tail_cursor": page.tail_cursor,
        "has_more": page.has_more,
        "is_tail": page.is_tail,
        "source_id": page.source_id,
    })))
}

pub async fn get_session_timeline_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<TimelineDetailQuery>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let query_service = ExternalQueryService::new(state.db.clone());
    ensure_session_exists(&query_service, &session_id).await?;

    let binding = AgentBindingService::new(state.db.clone())
        .primary_binding_for_session(&session_id)
        .await?
        .ok_or_else(|| {
            timeline_error(
                "not_ready",
                format!("session {session_id} has no agent binding"),
            )
        })?;

    let parser = PiJsonlParser::new();
    let detail = parser
        .timeline_item_detail(TimelineItemDetailRequest {
            session_id,
            source: resolve_binding_source(&binding)?,
            content_ref: query.content_ref,
        })
        .map_err(timeline_error_from_error)?;

    Ok(ok(json!({
        "binding_id": detail.binding_id,
        "content_ref": detail.content_ref,
        "content_type": detail.content_type,
        "text": detail.text,
        "size_bytes": detail.size_bytes,
    })))
}

fn resolve_binding_source(
    binding: &AgentBinding,
) -> Result<ResolvedAgentBinding, ExternalApiError> {
    PiAgentBindingResolver::new()
        .resolve(&AgentBindingResolveRequest {
            id: binding.id.clone(),
            session_id: binding.session_id.clone(),
            client_type: binding.client_type.clone(),
            launch_cwd: binding.launch_cwd.clone().into(),
            client_session_key: binding.client_session_key.clone(),
        })
        .map_err(timeline_error_from_error)
}

fn timeline_error_from_error(error: crate::error::Error) -> ExternalApiError {
    let message = error.to_string();
    if message.contains("source_unavailable:") {
        return timeline_error("source_unavailable", message);
    }
    if message.contains("cursor_invalid:") {
        return timeline_error("cursor_invalid", message);
    }
    if message.contains("content_ref_invalid:") {
        return timeline_error("content_ref_invalid", message);
    }
    ExternalApiError::from(error)
}

fn timeline_error(code: &'static str, message: impl Into<String>) -> ExternalApiError {
    ExternalApiError::custom(StatusCode::UNPROCESSABLE_ENTITY, code, message)
}
