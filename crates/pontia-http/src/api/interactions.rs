use super::response::{ApiError, ApiResponse, ok};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
};
use pontia_application::{
    AppState,
    interactions::{InteractionError, InteractionState},
};
use serde::Deserialize;
use serde_json::{Value, json};

impl From<InteractionError> for ApiError {
    fn from(error: InteractionError) -> Self {
        match error {
            InteractionError::NotFound => Self::custom(
                StatusCode::NOT_FOUND,
                "interaction_not_found",
                error.to_string(),
            ),
            InteractionError::InvalidOption => Self::custom(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_interaction_option",
                error.to_string(),
            ),
            InteractionError::Expired => Self::custom(
                StatusCode::CONFLICT,
                "interaction_expired",
                error.to_string(),
            ),
            InteractionError::Resolving => Self::custom(
                StatusCode::CONFLICT,
                "interaction_resolution_in_progress",
                error.to_string(),
            ),
            InteractionError::Other(error) => error.into(),
        }
    }
}
#[derive(Deserialize)]
pub struct InteractionQuery {
    state: Option<InteractionState>,
    limit: Option<i64>,
    cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseRequest {
    option_key: String,
}

pub async fn list_interactions(
    State(app): State<AppState>,
    Path(session): Path<String>,
    query: Result<Query<InteractionQuery>, QueryRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Query(query) = query.map_err(|e| ApiError::invalid_request(e.to_string()))?;
    let page = app
        .interactions()
        .list(
            &session,
            query.state,
            query.limit.unwrap_or(50),
            query.cursor.as_deref(),
        )
        .await?;
    Ok(ok(
        serde_json::to_value(page).map_err(pontia_core::Error::from)?
    ))
}
pub async fn get_interaction(
    State(app): State<AppState>,
    Path((session, id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    Ok(ok(
        json!({"interaction": app.interactions().get(&session,&id).await?}),
    ))
}
pub async fn respond_interaction(
    State(app): State<AppState>,
    Path((session, id)): Path<(String, String)>,
    body: Result<Json<ResponseRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(body) = body.map_err(|e| ApiError::invalid_request(e.to_string()))?;
    Ok(ok(
        json!({"interaction": app.interactions().respond(&app,&session,&id,&body.option_key).await?}),
    ))
}
