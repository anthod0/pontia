use super::{ApiError, response::ok};
use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use pontia_application::{
    AppState,
    session_files::{FileContentError, SessionFileService},
};
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncReadExt;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareFileRequest {
    path: std::path::PathBuf,
}

#[derive(Deserialize)]
pub struct FileListQuery {
    limit: Option<i64>,
    cursor: Option<String>,
}

pub async fn share_file(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(request): Json<ShareFileRequest>,
) -> Result<Response, ApiError> {
    let file = SessionFileService::new(state.db())
        .share(&session_id, request.path)
        .await?;
    Ok((StatusCode::CREATED, ok(json!({"file": file}))).into_response())
}

pub async fn list_session_files(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<FileListQuery>,
) -> Result<Response, ApiError> {
    let page = SessionFileService::new(state.db())
        .list(&session_id, query.limit, query.cursor.as_deref())
        .await?;
    Ok(ok(json!(page)).into_response())
}

pub async fn shared_file_content(
    State(state): State<AppState>,
    Path((session_id, file_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let content = SessionFileService::new(state.db())
        .content(&session_id, &file_id)
        .await
        .map_err(|e| match e {
            FileContentError::NotFound => ApiError::not_found(e.to_string()),
            FileContentError::PathChanged => {
                ApiError::custom(StatusCode::CONFLICT, "file_path_changed", e.to_string())
            }
            FileContentError::TooLarge => ApiError::custom(
                StatusCode::PAYLOAD_TOO_LARGE,
                "file_too_large",
                e.to_string(),
            ),
            FileContentError::Internal(error) => error.into(),
        })?;
    let mut reader = tokio::fs::File::from_std(content.file).take(content.size);
    let stream = async_stream::try_stream! {
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let count = reader.read(&mut buffer).await?;
            if count == 0 { break; }
            yield bytes::Bytes::copy_from_slice(&buffer[..count]);
        }
    };
    let stream: std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send>,
    > = Box::pin(stream);
    let encoded: String = content.name.bytes().map(|b| format!("%{b:02X}")).collect();
    Ok((
        [
            (header::CONTENT_TYPE, content.media_type.to_owned()),
            (header::CONTENT_LENGTH, content.size.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename*=UTF-8''{encoded}"),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
            (header::CACHE_CONTROL, "no-store".into()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}
