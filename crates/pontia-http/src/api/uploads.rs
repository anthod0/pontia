use super::{ApiError, response::ok};
use axum::{
    body::Bytes,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use pontia_application::{
    AppState,
    uploads::{Upload, UploadError, UploadService},
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct CreateQuery {
    name: String,
    size: String,
    sha256: String,
}
#[derive(Deserialize)]
pub struct OffsetQuery {
    offset: String,
}

fn number(value: &str) -> Result<u64, ApiError> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::invalid_request("Expected a nonnegative integer"));
    }
    value
        .parse()
        .map_err(|_| ApiError::invalid_request("Integer is out of range"))
}
fn query<T>(value: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    value
        .map(|Query(value)| value)
        .map_err(|error| ApiError::invalid_request(error.body_text()))
}
fn empty(body: &Bytes) -> Result<(), ApiError> {
    if !body.is_empty() {
        return Err(ApiError::invalid_request("Expected an empty body"));
    }
    Ok(())
}
fn upload_error(error: UploadError) -> ApiError {
    use UploadError::*;
    let (status, code) = match &error {
        Invalid => (StatusCode::BAD_REQUEST, "invalid_request"),
        NotFound => (StatusCode::NOT_FOUND, "upload_not_found"),
        TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "upload_too_large"),
        IdempotencyConflict => (StatusCode::CONFLICT, "upload_idempotency_conflict"),
        OffsetConflict => (StatusCode::CONFLICT, "upload_offset_conflict"),
        Incomplete => (StatusCode::CONFLICT, "upload_incomplete"),
        Completed => (StatusCode::CONFLICT, "upload_completed"),
        ChecksumMismatch => (StatusCode::UNPROCESSABLE_ENTITY, "upload_checksum_mismatch"),
        Io(_) | Record(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    };
    ApiError::custom(status, code, error.to_string())
}
async fn run<T: Send + 'static>(
    app: AppState,
    operation: impl FnOnce(UploadService) -> Result<T, UploadError> + Send + 'static,
) -> Result<T, ApiError> {
    let service = UploadService::new(app.pontia_home().to_owned());
    tokio::task::spawn_blocking(move || operation(service))
        .await
        .map_err(|error| {
            ApiError::custom(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                error.to_string(),
            )
        })?
        .map_err(upload_error)
}
fn response(upload: Upload) -> Response {
    ok(json!({ "upload": upload })).into_response()
}

pub async fn create_upload(
    State(app): State<AppState>,
    params: Result<Query<CreateQuery>, QueryRejection>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    empty(&body)?;
    let params = query(params)?;
    let size = number(&params.size)?;
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::invalid_request("Idempotency-Key is required"))?
        .to_owned();
    let (upload, created) = run(app, move |service| {
        service.create(&key, params.name, size, params.sha256)
    })
    .await?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        response(upload),
    )
        .into_response())
}
pub async fn get_upload(
    State(app): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    empty(&body)?;
    Ok(response(run(app, move |service| service.get(&id)).await?))
}
pub async fn append_upload(
    State(app): State<AppState>,
    Path(id): Path<String>,
    params: Result<Query<OffsetQuery>, QueryRejection>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let offset = number(&query(params)?.offset)?;
    if !headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .trim()
                .eq_ignore_ascii_case("application/octet-stream")
        })
    {
        return Err(ApiError::custom(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Expected application/octet-stream",
        ));
    }
    Ok(response(
        run(app, move |service| service.append(&id, offset, &body)).await?,
    ))
}
pub async fn complete_upload(
    State(app): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, ApiError> {
    empty(&body)?;
    Ok(response(
        run(app, move |service| service.complete(&id)).await?,
    ))
}
