use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};
use serde_json::Value;

pub(super) fn validate(event: &DomainEvent) -> Result<()> {
    let invalid = |message: &str| Error::Domain(message.into());
    if event.turn_id.is_some() {
        return Err(invalid(
            "Compaction facts are Session-level and cannot carry turn_id",
        ));
    }
    let payload = event
        .payload
        .as_object()
        .ok_or_else(|| invalid("Compaction payload must be an object"))?;
    if payload
        .get("operation_id")
        .and_then(Value::as_str)
        .is_none_or(|id| id.trim().is_empty())
    {
        return Err(invalid("payload.operation_id must be a non-empty string"));
    }
    if !matches!(
        payload.get("trigger").and_then(Value::as_str),
        Some("manual" | "threshold" | "overflow" | "unknown")
    ) {
        return Err(invalid(
            "payload.trigger must be manual, threshold, overflow, or unknown",
        ));
    }
    for (field, value) in payload {
        match field.as_str() {
            "operation_id" | "trigger" => {}
            "runtime_id" if value.as_str().is_some_and(|id| !id.trim().is_empty()) => {}
            "native_compaction_id"
                if event.event_type == EventType::SessionCompactionCompleted
                    && (value.is_null() || value.is_string()) => {}
            "tokens_before" | "tokens_after"
                if event.event_type == EventType::SessionCompactionCompleted
                    && (value.is_null() || value.as_u64().is_some()) => {}
            "aborted"
                if event.event_type == EventType::SessionCompactionFailed && value.is_boolean() => {
            }
            "error_message"
                if event.event_type == EventType::SessionCompactionFailed
                    && (value.is_null() || value.is_string()) => {}
            _ => {
                return Err(Error::Domain(format!(
                    "Unsupported or invalid compaction field: {field}"
                )));
            }
        }
    }
    if event.event_type == EventType::SessionCompactionFailed
        && !payload.get("aborted").is_some_and(Value::is_boolean)
    {
        return Err(invalid("payload.aborted must be a boolean"));
    }
    Ok(())
}

pub(super) fn normalize(kind: EventType, payload: &mut Value) {
    let fields: &[&str] = match kind {
        EventType::SessionCompactionCompleted => {
            &["native_compaction_id", "tokens_before", "tokens_after"]
        }
        EventType::SessionCompactionFailed => &["error_message"],
        _ => &[],
    };
    if let Some(payload) = payload.as_object_mut() {
        for field in fields {
            payload.entry(*field).or_insert(Value::Null);
        }
    }
}
