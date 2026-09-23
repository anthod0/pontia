use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};

pub fn normalize_payload(event_type: EventType, data: Value) -> Result<Value> {
    let object = data
        .as_object()
        .ok_or_else(|| Error::Domain("data must be a JSON object".to_string()))?;

    let payload = match event_type {
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
                "runtime_instance_id": object.get("runtime_instance_id").cloned().unwrap_or(Value::Null),
                "input": { "summary": input_summary },
                "timeline_anchor": { "previous_leaf_id": previous_leaf_id },
            });
            if let Some(inbox_message_id) = object
                .get("inbox_message_id")
                .or_else(|| data.pointer("/metadata/inbox_message_id"))
            {
                payload["metadata"] = json!({ "inbox_message_id": inbox_message_id });
            }
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
    Ok(payload)
}
