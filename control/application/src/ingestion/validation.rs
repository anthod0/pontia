use serde_json::Value;

use pontia_agent_clients::{
    TimelineSourceBehavior,
    raw_transcripts::{ManagedToolUseInput, TimelineItem},
};
use pontia_core::{
    domain::{DomainEvent, EventSource, EventType},
    error::{Error, Result},
};

#[derive(Clone, Default)]
pub struct InternalEventValidationService;

impl InternalEventValidationService {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, event: &DomainEvent) -> Result<()> {
        if event.event_type == EventType::TurnTimelineItem {
            let timeline_source = pontia_agent_clients::get_client_spec(&event.client_type)
                .map(|spec| spec.adapter.timeline_source)
                .unwrap_or(TimelineSourceBehavior::Unsupported);
            if timeline_source != TimelineSourceBehavior::ReportedEvents {
                return Err(Error::Domain(format!(
                    "{} does not accept reported timeline items",
                    event.client_type
                )));
            }
            validate_timeline_item(&event.payload)?;
        }

        if event.event_type == EventType::SessionReady && event.source == EventSource::AgentClient {
            let runtime_instance_id = event
                .payload
                .get("runtime_instance_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if runtime_instance_id.trim().is_empty() {
                return Err(Error::Domain(
                    "session.ready from agent_client requires payload.runtime_instance_id"
                        .to_string(),
                ));
            }
            if pontia_agent_clients::client_session_identity_required_on_ready(&event.client_type) {
                let client_session_key = event
                    .payload
                    .get("client_session_key")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if client_session_key.trim().is_empty() {
                    return Err(Error::Domain(format!(
                        "{} session.ready from agent_client requires payload.client_session_key",
                        event.client_type
                    )));
                }
            }
        }

        Ok(())
    }
}

fn validate_timeline_item(payload: &Value) -> Result<()> {
    let item: TimelineItem = serde_json::from_value(payload.clone())
        .map_err(|error| Error::Domain(format!("invalid turn.timeline_item payload: {error}")))?;
    require_bounded("item_id", &item.item_id, 256)?;
    if !matches!(item.kind.as_str(), "tool_call" | "tool_result") {
        return Err(Error::Domain(
            "turn.timeline_item kind must be tool_call or tool_result".to_string(),
        ));
    }
    if item.role != "assistant" {
        return Err(Error::Domain(
            "turn.timeline_item role must be assistant".to_string(),
        ));
    }
    require_optional_bounded("raw_kind", item.raw_kind.as_deref(), 100)?;
    require_optional_bounded("title", item.title.as_deref(), 200)?;
    let valid_status = matches!(
        (item.kind.as_str(), item.status.as_deref()),
        ("tool_call", Some("started"))
            | ("tool_result", Some("completed"))
            | ("tool_result", Some("error"))
    );
    if !valid_status {
        return Err(Error::Domain(
            "turn.timeline_item status does not match its kind".to_string(),
        ));
    }
    if item.occurred_at.is_some() {
        return Err(Error::Domain(
            "turn.timeline_item occurred_at is assigned by Pontia".to_string(),
        ));
    }
    require_bounded("content_preview", &item.content_preview, 1_000)?;
    if item.kind == "tool_result" && item.managed_tool_use.is_some() {
        return Err(Error::Domain(
            "turn.timeline_item tool results cannot carry managed tool input".to_string(),
        ));
    }

    if let Some(tool_use) = item.managed_tool_use {
        require_bounded("managed_tool_use.tool_name", &tool_use.tool_name, 100)?;
        match tool_use.input {
            ManagedToolUseInput::Read { path, .. }
            | ManagedToolUseInput::Write { path }
            | ManagedToolUseInput::Edit { path, .. } => {
                require_bounded("managed_tool_use.input.path", &path, 300)?;
            }
            ManagedToolUseInput::Bash { command, .. } => {
                require_bounded("managed_tool_use.input.command", &command, 300)?;
            }
        }
    }
    Ok(())
}

fn require_optional_bounded(field: &str, value: Option<&str>, max_chars: usize) -> Result<()> {
    if let Some(value) = value {
        require_bounded(field, value, max_chars)?;
    }
    Ok(())
}

fn require_bounded(field: &str, value: &str, max_chars: usize) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > max_chars {
        return Err(Error::Domain(format!(
            "turn.timeline_item {field} must contain 1 through {max_chars} characters"
        )));
    }
    Ok(())
}
