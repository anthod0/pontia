use std::{collections::HashMap, sync::Arc};

use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    error::{Error, Result},
    ids::new_event_id,
};
use serde::Deserialize;
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use tokio::sync::{Mutex, Notify, oneshot};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalWaitOutcome {
    ResolvedElsewhere,
    AcceptOnce,
    Reject,
    AlwaysAllow { permission_suggestion: Value },
}

impl ApprovalWaitOutcome {
    pub fn response_value(&self) -> Value {
        match self {
            Self::ResolvedElsewhere => Value::String("resolved_elsewhere".to_string()),
            Self::AcceptOnce => serde_json::json!({"decision": "accept_once"}),
            Self::Reject => serde_json::json!({"decision": "reject"}),
            Self::AlwaysAllow {
                permission_suggestion,
            } => serde_json::json!({
                "decision": "always_allow",
                "permission_suggestion": permission_suggestion,
            }),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApprovalDecisionRequest {
    AcceptOnce,
    Reject,
    AlwaysAllow { permission_suggestion: Value },
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
    _hook_input: Value,
    permission_suggestions: Vec<Value>,
    sender: oneshot::Sender<ApprovalWaitOutcome>,
}

struct ApprovalRequestOwner {
    session_id: String,
    turn_id: String,
}

#[derive(Debug, Clone, Copy)]
enum ApprovalAcceptScope {
    Once,
    Always,
    Unknown,
}

impl ApprovalAcceptScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Always => "always",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ClaudeDecision {
    Accept,
    Reject,
}

impl ClaudeDecision {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "accept" => Some(Self::Accept),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaudeDecisionSource {
    Config,
    Hook,
    UserPermanent,
    UserTemporary,
    UserAbort,
    Other,
}

impl From<&str> for ClaudeDecisionSource {
    fn from(value: &str) -> Self {
        match value {
            "config" => Self::Config,
            "hook" => Self::Hook,
            "user_permanent" => Self::UserPermanent,
            "user_temporary" => Self::UserTemporary,
            "user_abort" => Self::UserAbort,
            _ => Self::Other,
        }
    }
}

#[derive(Default)]
struct ApprovalCoordinatorState {
    active_requests: HashMap<String, ApprovalRequestOwner>,
    waiters: HashMap<String, ApprovalWaiter>,
    web_accept_scopes: HashMap<String, ApprovalAcceptScope>,
}

#[derive(Clone, Default)]
pub struct ApprovalCoordinator {
    state: Arc<Mutex<ApprovalCoordinatorState>>,
    changed: Arc<Notify>,
    finalization: Arc<Mutex<()>>,
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
        loop {
            let changed = self.changed.notified();
            let mut state = self.state.lock().await;
            if !state
                .active_requests
                .values()
                .any(|owner| owner.session_id == session_id && owner.turn_id == turn_id)
            {
                let (sender, receiver) = oneshot::channel();
                state.active_requests.insert(
                    request_event_id.clone(),
                    ApprovalRequestOwner {
                        session_id: session_id.clone(),
                        turn_id: turn_id.clone(),
                    },
                );
                state.waiters.insert(
                    request_event_id,
                    ApprovalWaiter {
                        session_id,
                        _hook_input: hook_input,
                        permission_suggestions,
                        sender,
                    },
                );
                return Ok(receiver);
            }
            drop(state);
            changed.await;
        }
    }

    async fn remove(&self, request_event_id: &str) {
        let mut state = self.state.lock().await;
        state.active_requests.remove(request_event_id);
        state.waiters.remove(request_event_id);
        state.web_accept_scopes.remove(request_event_id);
        drop(state);
        self.changed.notify_waiters();
    }

    async fn resolve_request(&self, request_event_id: &str) {
        let mut state = self.state.lock().await;
        state.active_requests.remove(request_event_id);
        state.web_accept_scopes.remove(request_event_id);
        if let Some(waiter) = state.waiters.remove(request_event_id) {
            let _ = waiter.sender.send(ApprovalWaitOutcome::ResolvedElsewhere);
        }
        drop(state);
        self.changed.notify_waiters();
    }

    async fn hook_accept_scope(&self, request_event_id: &str) -> Option<ApprovalAcceptScope> {
        self.state
            .lock()
            .await
            .web_accept_scopes
            .get(request_event_id)
            .copied()
    }

    async fn deliver_decision(
        &self,
        request_event_id: &str,
        session_id: &str,
        decision: ApprovalWaitOutcome,
    ) -> Result<()> {
        let mut state = self.state.lock().await;
        let waiter = state.waiters.get(request_event_id).ok_or_else(|| {
            Error::StateConflict("Approval request is no longer actionable".to_string())
        })?;
        if waiter.session_id != session_id {
            return Err(Error::StateConflict(
                "Approval request does not belong to the target Session".to_string(),
            ));
        }
        let decision = if let ApprovalWaitOutcome::AlwaysAllow {
            permission_suggestion,
        } = &decision
        {
            let original = waiter
                .permission_suggestions
                .iter()
                .find(|suggestion| *suggestion == permission_suggestion)
                .cloned()
                .ok_or_else(|| {
                    Error::StateConflict(
                        "permission suggestion does not exactly match the pending Approval"
                            .to_string(),
                    )
                })?;
            ApprovalWaitOutcome::AlwaysAllow {
                permission_suggestion: original,
            }
        } else {
            decision
        };
        let web_accept_scope = match &decision {
            ApprovalWaitOutcome::AcceptOnce => Some(ApprovalAcceptScope::Once),
            ApprovalWaitOutcome::AlwaysAllow { .. } => Some(ApprovalAcceptScope::Always),
            ApprovalWaitOutcome::ResolvedElsewhere | ApprovalWaitOutcome::Reject => None,
        };
        let waiter = state
            .waiters
            .remove(request_event_id)
            .expect("checked waiter must still exist while coordinator lock is held");
        if let Some(scope) = web_accept_scope {
            state
                .web_accept_scopes
                .insert(request_event_id.to_string(), scope);
        }
        if waiter.sender.send(decision).is_err() {
            state.web_accept_scopes.remove(request_event_id);
            return Err(Error::StateConflict(
                "Approval request is no longer actionable".to_string(),
            ));
        }
        Ok(())
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
            let state = self.state.lock().await;
            state
                .active_requests
                .iter()
                .filter(|(_, owner)| {
                    owner.session_id == event.session_id
                        && (event.event_type == EventType::SessionExited
                            || event.turn_id.as_deref() == Some(owner.turn_id.as_str()))
                })
                .map(|(request_event_id, _)| request_event_id.clone())
                .collect::<Vec<_>>()
        };
        let mut state = self.state.lock().await;
        for request_event_id in matching_ids {
            state.active_requests.remove(&request_event_id);
            state.web_accept_scopes.remove(&request_event_id);
            if let Some(waiter) = state.waiters.remove(&request_event_id) {
                let _ = waiter.sender.send(ApprovalWaitOutcome::ResolvedElsewhere);
            }
        }
        drop(state);
        self.changed.notify_waiters();
    }
}

#[derive(Clone)]
pub struct ApprovalCommandService {
    pool: SqlitePool,
    coordinator: ApprovalCoordinator,
}

impl ApprovalCommandService {
    pub fn new(pool: SqlitePool, coordinator: ApprovalCoordinator) -> Self {
        Self { pool, coordinator }
    }

    pub async fn decide(
        &self,
        session_id: &str,
        request_event_id: &str,
        request: ApprovalDecisionRequest,
    ) -> Result<Value> {
        let _finalization = self.coordinator.finalization.lock().await;
        let row = sqlx::query_as::<_, (String, String)>(
            r#"SELECT session_id, payload
               FROM events
               WHERE event_id = ? AND event_type = 'approval.requested'"#,
        )
        .bind(request_event_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            Error::StateConflict("Approval request is no longer actionable".to_string())
        })?;
        if row.0 != session_id {
            return Err(Error::StateConflict(
                "Approval request does not belong to the target Session".to_string(),
            ));
        }
        let payload: Value = serde_json::from_str(&row.1)?;
        let metadata =
            sqlx::query_scalar::<_, String>("SELECT metadata FROM sessions WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| {
                    Error::StateConflict("Approval request is no longer actionable".to_string())
                })?;
        let metadata: Value = serde_json::from_str(&metadata)?;
        let interaction = metadata.get("interaction");
        if interaction
            .and_then(|value| value.get("type"))
            .and_then(Value::as_str)
            != Some("approval")
            || interaction
                .and_then(|value| value.get("state"))
                .and_then(Value::as_str)
                != Some("awaiting")
            || interaction
                .and_then(|value| value.get("request_event_id"))
                .and_then(Value::as_str)
                != Some(request_event_id)
        {
            return Err(Error::StateConflict(
                "Approval request is no longer actionable".to_string(),
            ));
        }

        let outcome = match request {
            ApprovalDecisionRequest::AcceptOnce => ApprovalWaitOutcome::AcceptOnce,
            ApprovalDecisionRequest::Reject => ApprovalWaitOutcome::Reject,
            ApprovalDecisionRequest::AlwaysAllow {
                permission_suggestion,
            } => {
                let event_matches = payload
                    .get("permission_suggestions")
                    .and_then(Value::as_array)
                    .is_some_and(|suggestions| {
                        suggestions
                            .iter()
                            .any(|suggestion| suggestion == &permission_suggestion)
                    });
                if !event_matches {
                    return Err(Error::StateConflict(
                        "permission suggestion does not exactly match approval.requested"
                            .to_string(),
                    ));
                }
                ApprovalWaitOutcome::AlwaysAllow {
                    permission_suggestion,
                }
            }
        };
        self.coordinator
            .deliver_decision(request_event_id, session_id, outcome)
            .await?;
        Ok(serde_json::json!({
            "request_event_id": request_event_id,
            "delivered": true,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct ClaudeToolDecisionObservation {
    pub client_session_id: String,
    pub prompt_id: String,
    pub tool_name: String,
    pub tool_use_id: Option<String>,
    pub decision: String,
    pub decision_source: String,
}

#[derive(Clone)]
pub struct ApprovalObservationService {
    pool: SqlitePool,
    coordinator: ApprovalCoordinator,
}

impl ApprovalObservationService {
    pub fn new(pool: SqlitePool, coordinator: ApprovalCoordinator) -> Self {
        Self { pool, coordinator }
    }

    pub async fn observe_claude_tool_decision(
        &self,
        observation: ClaudeToolDecisionObservation,
    ) -> Result<bool> {
        let client_session_id = bounded_required("session.id", &observation.client_session_id)?;
        let prompt_id = bounded_required("prompt.id", &observation.prompt_id)?;
        let tool_name = bounded_required("tool_name", &observation.tool_name)?;
        let Some(decision) =
            ClaudeDecision::parse(bounded_required("decision", &observation.decision)?)
        else {
            return Ok(false);
        };
        let decision_source =
            ClaudeDecisionSource::from(bounded_required("source", &observation.decision_source)?);
        let tool_use_id = observation
            .tool_use_id
            .as_deref()
            .map(|value| bounded_required("tool_use_id", value))
            .transpose()?;

        let _finalization = self.coordinator.finalization.lock().await;
        let Some(context) = AgentBindingService::new(self.pool.clone())
            .current_turn_for_client_session("claude", client_session_id)
            .await?
        else {
            return Ok(false);
        };

        let unresolved = sqlx::query_as::<_, (String, String)>(
            r#"SELECT requested.event_id, requested.payload
               FROM events requested
               WHERE requested.session_id = ?
                 AND requested.turn_id = ?
                 AND requested.event_type = 'approval.requested'
                 AND NOT EXISTS (
                     SELECT 1
                     FROM events final
                     WHERE final.session_id = requested.session_id
                       AND final.turn_id = requested.turn_id
                       AND final.event_type IN (
                           'approval.accepted',
                           'approval.rejected',
                           'approval.cancelled'
                       )
                       AND json_extract(final.payload, '$.request_event_id') =
                           requested.event_id
                 )"#,
        )
        .bind(&context.session_id)
        .bind(&context.turn_id)
        .fetch_all(&self.pool)
        .await?;
        let [(request_event_id, requested_payload)] = unresolved.as_slice() else {
            return Ok(false);
        };
        let requested_payload: Value = serde_json::from_str(requested_payload)?;
        if requested_payload
            .get("client_session_id")
            .and_then(Value::as_str)
            != Some(client_session_id)
            || requested_payload.get("prompt_id").and_then(Value::as_str) != Some(prompt_id)
            || requested_payload.get("tool_name").and_then(Value::as_str) != Some(tool_name)
        {
            return Ok(false);
        }

        let (event_type, accepted_scope) = match decision {
            ClaudeDecision::Accept => {
                let scope = match decision_source {
                    ClaudeDecisionSource::UserTemporary => ApprovalAcceptScope::Once,
                    ClaudeDecisionSource::UserPermanent => ApprovalAcceptScope::Always,
                    ClaudeDecisionSource::Config => ApprovalAcceptScope::Unknown,
                    ClaudeDecisionSource::Hook => self
                        .coordinator
                        .hook_accept_scope(request_event_id)
                        .await
                        .unwrap_or(ApprovalAcceptScope::Unknown),
                    _ => return Ok(false),
                };
                (EventType::ApprovalAccepted, Some(scope))
            }
            ClaudeDecision::Reject if decision_source == ClaudeDecisionSource::UserAbort => {
                (EventType::ApprovalCancelled, None)
            }
            ClaudeDecision::Reject => (EventType::ApprovalRejected, None),
        };

        let mut payload = Map::new();
        payload.insert(
            "request_event_id".to_string(),
            Value::String(request_event_id.clone()),
        );
        payload.insert(
            "client_session_id".to_string(),
            Value::String(client_session_id.to_string()),
        );
        payload.insert(
            "prompt_id".to_string(),
            Value::String(prompt_id.to_string()),
        );
        payload.insert(
            "tool_name".to_string(),
            Value::String(tool_name.to_string()),
        );
        if let Some(tool_use_id) = tool_use_id {
            payload.insert(
                "tool_use_id".to_string(),
                Value::String(tool_use_id.to_string()),
            );
        }
        if let Some(scope) = accepted_scope {
            payload.insert(
                "scope".to_string(),
                Value::String(scope.as_str().to_string()),
            );
        }

        EventIngestService::new(self.pool.clone())
            .ingest_reported_event(ReportedEvent::new(
                new_event_id().to_string(),
                context.session_id,
                Some(context.turn_id),
                EventSource::AgentClient,
                "claude".to_string(),
                event_type,
                Value::Object(payload),
            ))
            .await?;
        self.coordinator.resolve_request(request_event_id).await;
        Ok(true)
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

    #[tokio::test]
    async fn concurrent_decisions_only_wake_a_waiter_once() {
        let coordinator = ApprovalCoordinator::default();
        let receiver = coordinator
            .register(
                "evt_approval".to_string(),
                "sess_approval".to_string(),
                "turn_approval".to_string(),
                json!({}),
                Vec::new(),
            )
            .await
            .unwrap();

        let accept = coordinator.deliver_decision(
            "evt_approval",
            "sess_approval",
            ApprovalWaitOutcome::AcceptOnce,
        );
        let reject = coordinator.deliver_decision(
            "evt_approval",
            "sess_approval",
            ApprovalWaitOutcome::Reject,
        );
        let (accept, reject) = tokio::join!(accept, reject);

        assert_ne!(accept.is_ok(), reject.is_ok());
        assert!(matches!(
            receiver.await.unwrap(),
            ApprovalWaitOutcome::AcceptOnce | ApprovalWaitOutcome::Reject
        ));
    }
}
