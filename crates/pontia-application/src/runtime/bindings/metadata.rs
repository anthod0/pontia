use serde_json::{Value, json};

use super::{RuntimeBindingUpsertRequest, request::non_empty};

pub(super) fn agent_binding_metadata(request: &RuntimeBindingUpsertRequest) -> Value {
    let mut metadata = serde_json::Map::new();
    insert_optional(
        &mut metadata,
        "client_session_file",
        &request.client_session_file,
    );
    insert_optional(
        &mut metadata,
        "client_session_dir",
        &request.client_session_dir,
    );
    insert_optional(&mut metadata, "client_cwd", &request.client_cwd);
    Value::Object(metadata)
}

fn insert_optional(
    metadata: &mut serde_json::Map<String, Value>,
    key: &str,
    value: &Option<String>,
) {
    if let Some(value) = non_empty(value.as_deref()) {
        metadata.insert(key.to_string(), json!(value));
    }
}
