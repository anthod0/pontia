use std::{collections::HashMap, sync::Arc};

use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    error::{Error, Result},
    ids::new_event_id,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use tokio::sync::{Mutex, oneshot};

use crate::{AgentBindingService, EventIngestService};

pub const MAX_PERMISSION_SUGGESTIONS: usize = 8;
pub const MAX_PERMISSION_RULES: usize = 16;
pub const MAX_PERMISSION_DIRECTORIES: usize = 16;
pub const MAX_APPROVAL_STRING_CHARS: usize = 512;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRegistrationRequest {
    pub session_id: String,
    pub prompt_id: Option<String>,
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Value,
    #[serde(default)]
    pub permission_suggestions: Vec<Value>,
    #[serde(default)]
    pub hook_input: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalWaitOutcome {
    ResolvedElsewhere,
}

pub struct PendingApproval {
    pub request_event_id: String,
    pub session_id: String,
    pub turn_id: String,
    receiver: oneshot::Receiver<ApprovalWaitOutcome>,
}

impl PendingApproval {
    pub async fn wait(self) -> ApprovalWaitOutcome {
        self.receiver
            .await
            .unwrap_or(ApprovalWaitOutcome::ResolvedElsewhere)
    }
}

struct ApprovalWaiter {
    session_id: String,
    turn_id: String,
    _hook_input: Value,
    _permission_suggestions: Vec<Value>,
    sender: oneshot::Sender<ApprovalWaitOutcome>,
}

#[derive(Clone, Default)]
pub struct ApprovalCoordinator {
    waiters: Arc<Mutex<HashMap<String, ApprovalWaiter>>>,
}

impl ApprovalCoordinator {
    async fn register(
        &self,
        request_event_id: String,
        session_id: String,
        turn_id: String,
        hook_input: Value,
        permission_suggestions: Vec<Value>,
    ) -> Result<oneshot::Receiver<ApprovalWaitOutcome>> {
        let mut waiters = self.waiters.lock().await;
        if waiters
            .values()
            .any(|waiter| waiter.session_id == session_id && waiter.turn_id == turn_id)
        {
            return Err(Error::StateConflict(
                "the active Turn already has a pending Approval".to_string(),
            ));
        }
        let (sender, receiver) = oneshot::channel();
        waiters.insert(
            request_event_id,
            ApprovalWaiter {
                session_id,
                turn_id,
                _hook_input: hook_input,
                _permission_suggestions: permission_suggestions,
                sender,
            },
        );
        Ok(receiver)
    }

    async fn remove(&self, request_event_id: &str) {
        self.waiters.lock().await.remove(request_event_id);
    }

    async fn resolve_request(&self, request_event_id: &str) {
        if let Some(waiter) = self.waiters.lock().await.remove(request_event_id) {
            let _ = waiter.sender.send(ApprovalWaitOutcome::ResolvedElsewhere);
        }
    }

    pub async fn resolve_terminal_event(&self, event: &ReportedEvent) {
        let turn_terminal = matches!(
            event.event_type,
            EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
                | EventType::TurnAbandoned
                | EventType::TurnInterrupted
        );
        if !turn_terminal && event.event_type != EventType::SessionExited {
            return;
        }

        let matching_ids = {
            let waiters = self.waiters.lock().await;
            waiters
                .iter()
                .filter(|(_, waiter)| {
                    waiter.session_id == event.session_id
                        && (event.event_type == EventType::SessionExited
                            || event.turn_id.as_deref() == Some(waiter.turn_id.as_str()))
                })
                .map(|(request_event_id, _)| request_event_id.clone())
                .collect::<Vec<_>>()
        };
        let mut waiters = self.waiters.lock().await;
        for request_event_id in matching_ids {
            if let Some(waiter) = waiters.remove(&request_event_id) {
                let _ = waiter.sender.send(ApprovalWaitOutcome::ResolvedElsewhere);
            }
        }
    }
}

#[derive(Clone)]
pub struct ApprovalRegistrationService {
    pool: SqlitePool,
    coordinator: ApprovalCoordinator,
}

impl ApprovalRegistrationService {
    pub fn new(pool: SqlitePool, coordinator: ApprovalCoordinator) -> Self {
        Self { pool, coordinator }
    }

    pub async fn register(
        &self,
        request: ApprovalRegistrationRequest,
    ) -> Result<Option<PendingApproval>> {
        let client_session_key = bounded_required("session_id", &request.session_id)?;
        let tool_name = bounded_required("tool_name", &request.tool_name)?;
        let prompt_id = request
            .prompt_id
            .as_deref()
            .map(|value| bounded_required("prompt_id", value))
            .transpose()?;
        if !request.tool_input.is_object() {
            return Err(Error::Domain(
                "tool_input must be a JSON object".to_string(),
            ));
        }
        if !request.hook_input.is_object() {
            return Err(Error::Domain(
                "hook_input must be a JSON object".to_string(),
            ));
        }

        let Some(context) = AgentBindingService::new(self.pool.clone())
            .current_turn_for_client_session("claude", client_session_key)
            .await?
        else {
            return Ok(None);
        };

        let permission_suggestions = request
            .permission_suggestions
            .iter()
            .take(MAX_PERMISSION_SUGGESTIONS)
            .filter(|suggestion| valid_permission_suggestion(suggestion))
            .cloned()
            .collect::<Vec<_>>();
        let request_event_id = new_event_id().to_string();
        let mut payload = Map::new();
        payload.insert(
            "client_session_id".to_string(),
            Value::String(client_session_key.to_string()),
        );
        if let Some(prompt_id) = prompt_id {
            payload.insert(
                "prompt_id".to_string(),
                Value::String(prompt_id.to_string()),
            );
        }
        payload.insert(
            "tool_name".to_string(),
            Value::String(tool_name.to_string()),
        );
        payload.insert(
            "permission_suggestions".to_string(),
            Value::Array(permission_suggestions.clone()),
        );

        let receiver = self
            .coordinator
            .register(
                request_event_id.clone(),
                context.session_id.clone(),
                context.turn_id.clone(),
                request.hook_input,
                request.permission_suggestions,
            )
            .await?;
        let event = ReportedEvent::new(
            request_event_id.clone(),
            context.session_id.clone(),
            Some(context.turn_id.clone()),
            EventSource::AgentClient,
            "claude".to_string(),
            EventType::ApprovalRequested,
            Value::Object(payload),
        );
        if let Err(error) = EventIngestService::new(self.pool.clone())
            .ingest_reported_event(event)
            .await
        {
            self.coordinator.remove(&request_event_id).await;
            return Err(error);
        }
        watch_terminal_projection(
            self.pool.clone(),
            self.coordinator.clone(),
            request_event_id.clone(),
            context.session_id.clone(),
            context.turn_id.clone(),
        );

        Ok(Some(PendingApproval {
            request_event_id,
            session_id: context.session_id,
            turn_id: context.turn_id,
            receiver,
        }))
    }
}

fn watch_terminal_projection(
    pool: SqlitePool,
    coordinator: ApprovalCoordinator,
    request_event_id: String,
    session_id: String,
    turn_id: String,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let states = sqlx::query_as::<_, (String, String)>(
                r#"SELECT s.state, t.state
                   FROM sessions s
                   JOIN turns t ON t.session_id = s.session_id
                   WHERE s.session_id = ? AND t.turn_id = ?"#,
            )
            .bind(&session_id)
            .bind(&turn_id)
            .fetch_optional(&pool)
            .await;
            let Ok(Some((session_state, turn_state))) = states else {
                continue;
            };
            if matches!(session_state.as_str(), "exited" | "error")
                || matches!(
                    turn_state.as_str(),
                    "completed" | "failed" | "interrupted" | "abandoned"
                )
            {
                coordinator.resolve_request(&request_event_id).await;
                break;
            }
        }
    });
}

fn bounded_required<'a>(field: &str, value: &'a str) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::Domain(format!("{field} is required")));
    }
    if value.chars().count() > MAX_APPROVAL_STRING_CHARS {
        return Err(Error::Domain(format!(
            "{field} exceeds {MAX_APPROVAL_STRING_CHARS} characters"
        )));
    }
    Ok(value)
}

fn valid_permission_suggestion(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let Some(kind) = object.get("type").and_then(Value::as_str) else {
        return false;
    };
    match kind {
        "addRules" | "replaceRules" | "removeRules" => {
            has_exact_keys(object, &["type", "rules", "behavior", "destination"])
                && valid_rules(object.get("rules"))
                && matches!(
                    object.get("behavior").and_then(Value::as_str),
                    Some("allow" | "deny" | "ask")
                )
                && valid_destination(object.get("destination"))
        }
        "setMode" => {
            has_exact_keys(object, &["type", "mode", "destination"])
                && matches!(
                    object.get("mode").and_then(Value::as_str),
                    Some(
                        "default"
                            | "auto"
                            | "acceptEdits"
                            | "dontAsk"
                            | "bypassPermissions"
                            | "plan"
                            | "manual"
                    )
                )
                && valid_destination(object.get("destination"))
        }
        "addDirectories" => {
            has_exact_keys(object, &["type", "directories", "destination"])
                && valid_directories(object.get("directories"))
                && valid_destination(object.get("destination"))
        }
        _ => false,
    }
}

fn valid_rules(value: Option<&Value>) -> bool {
    let Some(rules) = value.and_then(Value::as_array) else {
        return false;
    };
    !rules.is_empty()
        && rules.len() <= MAX_PERMISSION_RULES
        && rules.iter().all(|rule| {
            let Some(rule) = rule.as_object() else {
                return false;
            };
            let keys_valid = rule
                .keys()
                .all(|key| key == "toolName" || key == "ruleContent")
                && rule.contains_key("toolName");
            keys_valid
                && valid_bounded_string(rule.get("toolName"))
                && rule
                    .get("ruleContent")
                    .is_none_or(|value| valid_bounded_string(Some(value)))
        })
}

fn valid_directories(value: Option<&Value>) -> bool {
    let Some(directories) = value.and_then(Value::as_array) else {
        return false;
    };
    !directories.is_empty()
        && directories.len() <= MAX_PERMISSION_DIRECTORIES
        && directories
            .iter()
            .all(|directory| valid_bounded_string(Some(directory)))
}

fn valid_bounded_string(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str).is_some_and(|value| {
        !value.is_empty() && value.chars().count() <= MAX_APPROVAL_STRING_CHARS
    })
}

fn valid_destination(value: Option<&Value>) -> bool {
    matches!(
        value.and_then(Value::as_str),
        Some("userSettings" | "projectSettings" | "localSettings" | "session")
    )
}

fn has_exact_keys(object: &Map<String, Value>, keys: &[&str]) -> bool {
    object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn suggestion_validation_preserves_only_exact_bounded_schema() {
        let valid = json!({
            "type": "addRules",
            "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
            "behavior": "allow",
            "destination": "localSettings"
        });
        assert!(valid_permission_suggestion(&valid));

        let mut unknown = valid.clone();
        unknown["unexpected"] = json!(true);
        assert!(!valid_permission_suggestion(&unknown));

        let oversized = json!({
            "type": "addRules",
            "rules": [{"toolName": "Bash", "ruleContent": "x".repeat(MAX_APPROVAL_STRING_CHARS + 1)}],
            "behavior": "allow",
            "destination": "localSettings"
        });
        assert!(!valid_permission_suggestion(&oversized));

        let too_many_rules = json!({
            "type": "addRules",
            "rules": (0..=MAX_PERMISSION_RULES)
                .map(|index| json!({"toolName": "Bash", "ruleContent": format!("command {index}")}))
                .collect::<Vec<_>>(),
            "behavior": "allow",
            "destination": "localSettings"
        });
        assert!(!valid_permission_suggestion(&too_many_rules));
    }
}
