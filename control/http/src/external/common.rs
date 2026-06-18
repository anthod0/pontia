use axum::{
    Json,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};

use pontia_application::{AppState, ExternalQueryService};
use pontia_core::error::Error;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    data: Option<T>,
    meta: Value,
    error: Option<ApiErrorBody>,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    code: &'static str,
    message: String,
}

pub(super) async fn ensure_session_exists(
    service: &ExternalQueryService,
    session_id: &str,
) -> Result<(), ExternalApiError> {
    service
        .get_session(session_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("session {session_id} not found")))?;
    Ok(())
}

pub(super) fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<(), ExternalApiError> {
    let Some(expected) = state.external_api_token() else {
        return Err(ExternalApiError::authentication_failed(
            "external API token is not configured",
        ));
    };

    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);

    if authorized {
        Ok(())
    } else {
        Err(ExternalApiError::authentication_failed(
            "missing or invalid bearer token",
        ))
    }
}

pub(super) fn idempotency_key(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
}

pub(super) fn ok(data: Value) -> Json<ApiResponse<Value>> {
    Json(ApiResponse {
        data: Some(data),
        meta: json!({}),
        error: None,
    })
}

#[derive(Debug)]
pub struct ExternalApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ExternalApiError {
    fn authentication_failed(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "authentication_failed",
            message: message.into(),
        }
    }

    pub(super) fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }

    fn state_conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "state_conflict",
            message: message.into(),
        }
    }

    fn capability_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "capability_unavailable",
            message: message.into(),
        }
    }

    pub(super) fn custom(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

impl From<Error> for ExternalApiError {
    fn from(error: Error) -> Self {
        match error {
            Error::StateConflict(message) => Self::state_conflict(message),
            Error::Conflict { code, message } => Self {
                status: StatusCode::CONFLICT,
                code,
                message,
            },
            Error::CapabilityUnavailable(message) => Self::capability_unavailable(message),
            Error::NotFound(message) => Self::not_found(message),
            Error::Domain(message) => Self {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_request",
                message,
            },
            other => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                message: other.to_string(),
            },
        }
    }
}

impl IntoResponse for ExternalApiError {
    fn into_response(self) -> Response {
        let body = Json(ApiResponse::<Value> {
            data: None,
            meta: json!({}),
            error: Some(ApiErrorBody {
                code: self.code,
                message: self.message,
            }),
        });
        (self.status, body).into_response()
    }
}
