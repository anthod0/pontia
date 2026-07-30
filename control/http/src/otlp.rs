use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};
use pontia_application::{AppState, ApprovalObservationService, ClaudeToolDecisionObservation};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::internal::{ApiError, authenticate_internal_token};

const CLAUDE_TOOL_DECISION_EVENT: &str = "claude_code.tool_decision";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportLogsServiceRequest {
    #[serde(default)]
    resource_logs: Vec<ResourceLogs>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceLogs {
    resource: Option<Resource>,
    #[serde(default)]
    scope_logs: Vec<ScopeLogs>,
}

#[derive(Debug, Deserialize)]
struct Resource {
    #[serde(default)]
    attributes: Vec<KeyValue>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScopeLogs {
    #[serde(default)]
    log_records: Vec<LogRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogRecord {
    event_name: Option<String>,
    body: Option<AnyValue>,
    #[serde(default)]
    attributes: Vec<KeyValue>,
}

#[derive(Debug, Deserialize)]
struct KeyValue {
    key: String,
    value: AnyValue,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnyValue {
    string_value: Option<String>,
}

pub async fn post_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<ExportLogsServiceRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    authenticate_internal_token(&state, &headers, "Internal OTLP token is not configured")?;
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    let service = ApprovalObservationService::new(state.db(), state.approvals());

    for resource_logs in request.resource_logs {
        let resource_attributes = resource_logs
            .resource
            .map(|resource| resource.attributes)
            .unwrap_or_default();
        for scope_logs in resource_logs.scope_logs {
            for record in scope_logs.log_records {
                if !is_claude_tool_decision(&record) {
                    continue;
                }
                let Some(observation) =
                    tool_decision_observation(&record.attributes, &resource_attributes)
                else {
                    continue;
                };
                service.observe_claude_tool_decision(observation).await?;
            }
        }
    }

    Ok(Json(json!({})))
}

fn is_claude_tool_decision(record: &LogRecord) -> bool {
    match record.event_name.as_deref() {
        Some(event_name) => event_name == CLAUDE_TOOL_DECISION_EVENT,
        None => record.body.as_ref().and_then(string_value) == Some(CLAUDE_TOOL_DECISION_EVENT),
    }
}

fn tool_decision_observation(
    record_attributes: &[KeyValue],
    resource_attributes: &[KeyValue],
) -> Option<ClaudeToolDecisionObservation> {
    let attribute = |key| {
        attribute_string(record_attributes, key)
            .or_else(|| attribute_string(resource_attributes, key))
    };
    Some(ClaudeToolDecisionObservation {
        client_session_id: attribute("session.id")?.to_string(),
        prompt_id: attribute_string(record_attributes, "prompt.id")?.to_string(),
        tool_name: attribute_string(record_attributes, "tool_name")?.to_string(),
        tool_use_id: attribute_string(record_attributes, "tool_use_id").map(ToString::to_string),
        decision: attribute_string(record_attributes, "decision")?.to_string(),
        decision_source: attribute_string(record_attributes, "source")?.to_string(),
    })
}

fn attribute_string<'a>(attributes: &'a [KeyValue], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .and_then(|attribute| string_value(&attribute.value))
}

fn string_value(value: &AnyValue) -> Option<&str> {
    value.string_value.as_deref()
}
