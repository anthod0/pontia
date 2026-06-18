use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::{Value, json};

use pontia_application::{AppState, RawTranscriptService, RawTranscriptServiceError};

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
    let page = RawTranscriptService::new(state.db())
        .timeline_page(session_id, query.before, query.after, query.limit)
        .await
        .map_err(timeline_service_error)?;

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
    let detail = RawTranscriptService::new(state.db())
        .timeline_item_detail(session_id, query.content_ref)
        .await
        .map_err(timeline_service_error)?;

    Ok(ok(json!({
        "binding_id": detail.binding_id,
        "content_ref": detail.content_ref,
        "content_type": detail.content_type,
        "text": detail.text,
        "size_bytes": detail.size_bytes,
    })))
}

fn timeline_service_error(error: RawTranscriptServiceError) -> ExternalApiError {
    match error {
        RawTranscriptServiceError::NotFound(message) => ExternalApiError::not_found(message),
        RawTranscriptServiceError::Timeline { code, message } => {
            timeline_error(code.as_str(), message)
        }
        RawTranscriptServiceError::Inner(error) => ExternalApiError::from(error),
    }
}

fn timeline_error(code: &'static str, message: impl Into<String>) -> ExternalApiError {
    ExternalApiError::custom(StatusCode::UNPROCESSABLE_ENTITY, code, message)
}
