use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};

pub fn normalize_payload(event_type: EventType, data: Value) -> Result<Value> {
    let object = data
        .as_object()
        .ok_or_else(|| Error::Domain("data must be a JSON object".to_string()))?;

    for anchor in ["previous_leaf_id", "terminal_leaf_id"] {
        if let Some(value) = object
            .get(anchor)
            .or_else(|| data.get("timeline_anchor").and_then(|v| v.get(anchor)))
            && !value.is_null()
            && !value
                .as_str()
                .is_some_and(|id| !id.trim().is_empty() && id.len() <= 512)
        {
            return Err(Error::Domain("invalid Pi native timeline anchor".into()));
        }
    }
    let runtime_id = object.get("runtime_id").cloned();
    let mut payload = match event_type {
        EventType::TurnStarted => {
            let input_summary = object
                .get("input_summary")
                .or_else(|| data.pointer("/input/summary"))
                .cloned()
                .unwrap_or(Value::Null);
            let previous_leaf_id = object
                .get("previous_leaf_id")
                .or_else(|| data.pointer("/timeline_anchor/previous_leaf_id"))
                .cloned()
                .unwrap_or(Value::Null);
            let mut payload = json!({
                "input": { "summary": input_summary },
                "timeline_anchor": { "previous_leaf_id": previous_leaf_id },
            });
            if let Some(topology_context) = object.get("topology_context") {
                payload["topology_context"] = topology_context.clone();
            }
            payload
        }
        EventType::TurnOutput => json!({
            "output": {
                "summary": object
                    .get("output_summary")
                    .or_else(|| data.pointer("/output/summary"))
                    .cloned()
                    .unwrap_or(Value::Null),
            }
        }),
        EventType::TurnCompleted | EventType::TurnInterrupted => json!({
            "timeline_anchor": {
                "terminal_leaf_id": object
                    .get("terminal_leaf_id")
                    .or_else(|| data.pointer("/timeline_anchor/terminal_leaf_id"))
                    .cloned()
                    .unwrap_or(Value::Null),
            }
        }),
        EventType::TurnFailed => json!({
            "failure": {
                "message": object
                    .get("failure_message")
                    .or_else(|| data.pointer("/failure/message"))
                    .cloned()
                    .unwrap_or(Value::Null),
            },
            "timeline_anchor": {
                "terminal_leaf_id": object
                    .get("terminal_leaf_id")
                    .or_else(|| data.pointer("/timeline_anchor/terminal_leaf_id"))
                    .cloned()
                    .unwrap_or(Value::Null),
            }
        }),
        _ => data,
    };
    if let Some(runtime_id) = runtime_id {
        payload["runtime_id"] = runtime_id;
    }
    Ok(payload)
}
