use super::ApiError;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use pontia_application::{
    AppState,
    session_files::{FileContent, FileContentError, SessionFileService},
};
use serde::Deserialize;
use tokio::io::AsyncReadExt;

#[derive(Deserialize)]
pub struct FileContentQuery {
    path: std::path::PathBuf,
}

pub async fn path_file_content(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Query(query): Query<FileContentQuery>,
) -> Result<Response, ApiError> {
    let content = SessionFileService::new(state.db())
        .path_content(&session_id, query.path)
        .await;
    file_response(content)
}

fn file_response(content: Result<FileContent, FileContentError>) -> Result<Response, ApiError> {
    let content = content.map_err(|e| match e {
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
