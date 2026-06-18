use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    agent_clients::{TranscriptBehavior, get_client_definition},
    application::{
        AgentBinding, AgentBindingResolveRequest, AgentBindingResolver, AgentBindingService,
        AppState, ExternalQueryService, PiAgentBindingResolver, PiJsonlParser, RawTranscriptParser,
        ResolvedAgentBinding, TimelineItemDetailRequest, TimelinePageRequest,
    },
};

use super::common::{ApiResponse, ExternalApiError, authenticate, ok};

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    before: Option<String>,
    after: Option<String>,
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
    let query_service = ExternalQueryService::new(state.db());
    let session = query_service
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("session {session_id} not found")))?;

    let binding_service = AgentBindingService::new(state.db());
    let binding = binding_service
        .primary_binding_for_session(&session_id)
        .await?
        .ok_or_else(|| {
            timeline_error(
                "not_ready",
                format!("session {session_id} has no agent binding"),
            )
        })?;

    let transcript = transcript_backend(&binding)?;
    let source = resolve_binding_source(&binding, transcript, &session.state).await?;
    let page = match transcript {
        TranscriptBehavior::PiJsonl => PiJsonlParser::new()
            .timeline_page(TimelinePageRequest {
                session_id,
                source,
                before: query.before,
                after: query.after,
                limit: query.limit,
            })
            .map_err(|error| {
                timeline_error_from_error(error, binding.discovered, &session.state)
            })?,
        TranscriptBehavior::Unsupported => unreachable!("unsupported transcript checked above"),
    };
    if !binding.discovered {
        binding_service.mark_discovered(&binding.id).await?;
    }

    Ok(ok(json!({
        "session_id": page.session_id,
        "binding_id": page.binding_id,
        "items": page.items,
        "head_cursor": page.head_cursor,
        "tail_cursor": page.tail_cursor,
        "has_more": page.has_more,
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
    let query_service = ExternalQueryService::new(state.db());
    let session = query_service
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("session {session_id} not found")))?;

    let binding_service = AgentBindingService::new(state.db());
    let binding = binding_service
        .primary_binding_for_session(&session_id)
        .await?
        .ok_or_else(|| {
            timeline_error(
                "not_ready",
                format!("session {session_id} has no agent binding"),
            )
        })?;

    let transcript = transcript_backend(&binding)?;
    let source = resolve_binding_source(&binding, transcript, &session.state).await?;
    let detail = match transcript {
        TranscriptBehavior::PiJsonl => PiJsonlParser::new()
            .timeline_item_detail(TimelineItemDetailRequest {
                session_id,
                source,
                content_ref: query.content_ref,
            })
            .map_err(|error| {
                timeline_error_from_error(error, binding.discovered, &session.state)
            })?,
        TranscriptBehavior::Unsupported => unreachable!("unsupported transcript checked above"),
    };
    if !binding.discovered {
        binding_service.mark_discovered(&binding.id).await?;
    }

    Ok(ok(json!({
        "binding_id": detail.binding_id,
        "content_ref": detail.content_ref,
        "content_type": detail.content_type,
        "text": detail.text,
        "size_bytes": detail.size_bytes,
    })))
}

fn transcript_backend(binding: &AgentBinding) -> Result<TranscriptBehavior, ExternalApiError> {
    let behavior = get_client_definition(&binding.client_type)
        .map(|definition| definition.backend.transcript)
        .unwrap_or(TranscriptBehavior::Unsupported);
    if behavior == TranscriptBehavior::Unsupported {
        return Err(timeline_error(
            "capability_unavailable",
            format!(
                "{} client does not support backend transcript timeline",
                binding.client_type
            ),
        ));
    }
    Ok(behavior)
}

async fn resolve_binding_source(
    binding: &AgentBinding,
    transcript: TranscriptBehavior,
    session_state: &str,
) -> Result<ResolvedAgentBinding, ExternalApiError> {
    match transcript {
        TranscriptBehavior::PiJsonl => PiAgentBindingResolver::new()
            .resolve(&AgentBindingResolveRequest {
                id: binding.id.clone(),
                session_id: binding.session_id.clone(),
                client_type: binding.client_type.clone(),
                launch_cwd: binding.launch_cwd.clone().into(),
                client_session_key: binding.client_session_key.clone(),
            })
            .map_err(|error| {
                timeline_error_from_binding_error(error, binding.discovered, session_state)
            }),
        TranscriptBehavior::Unsupported => Err(timeline_error(
            "capability_unavailable",
            format!(
                "{} client does not support backend transcript timeline",
                binding.client_type
            ),
        )),
    }
}

fn timeline_error_from_binding_error(
    error: crate::error::Error,
    discovered: bool,
    session_state: &str,
) -> ExternalApiError {
    let message = error.to_string();
    if message.contains("source_unavailable:") {
        return timeline_error(source_unavailable_code(discovered, session_state), message);
    }
    timeline_error_from_error(error, discovered, session_state)
}

fn timeline_error_from_error(
    error: crate::error::Error,
    discovered: bool,
    session_state: &str,
) -> ExternalApiError {
    let message = error.to_string();
    if message.contains("source_unavailable:") {
        return timeline_error(source_unavailable_code(discovered, session_state), message);
    }
    if message.contains("cursor_invalid:") {
        return timeline_error("cursor_invalid", message);
    }
    if message.contains("content_ref_invalid:") {
        return timeline_error("content_ref_invalid", message);
    }
    ExternalApiError::from(error)
}

fn source_unavailable_code(discovered: bool, session_state: &str) -> &'static str {
    if discovered && matches!(session_state, "exited" | "error") {
        "source_unavailable"
    } else {
        "not_ready"
    }
}

fn timeline_error(code: &'static str, message: impl Into<String>) -> ExternalApiError {
    ExternalApiError::custom(StatusCode::UNPROCESSABLE_ENTITY, code, message)
}
