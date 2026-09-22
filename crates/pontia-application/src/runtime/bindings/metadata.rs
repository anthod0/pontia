use serde_json::{Value, json};

use super::{RuntimeBindingTmuxRequest, RuntimeBindingUpsertRequest, request::non_empty};

pub(super) fn runtime_diagnostics(
    log_dir: &str,
    runtime_log: &str,
    hook_log_metadata: Option<(&str, &str)>,
) -> Value {
    let mut diagnostics = serde_json::Map::new();
    diagnostics.insert("log_dir".to_string(), json!(log_dir));
    diagnostics.insert("runtime_log".to_string(), json!(runtime_log));
    if let Some((metadata_key, hook_log_path)) = hook_log_metadata {
        diagnostics.insert(metadata_key.to_string(), json!(hook_log_path));
    }
    Value::Object(diagnostics)
}

pub(super) fn adapter_details(request: &RuntimeBindingUpsertRequest) -> Value {
    let mut details = serde_json::Map::new();
    if let Some(tmux) = &request.tmux {
        details.insert("tmux".to_string(), tmux_details(tmux));
    }
    Value::Object(details)
}

fn tmux_details(tmux: &RuntimeBindingTmuxRequest) -> Value {
    let mut details = serde_json::Map::new();
    insert_optional(&mut details, "session_id", &tmux.session_id);
    insert_optional(&mut details, "session_name", &tmux.session_name);
    insert_optional(&mut details, "window_id", &tmux.window_id);
    if let Some(window_index) = tmux.window_index {
        details.insert("window_index".to_string(), json!(window_index));
    }
    insert_optional(&mut details, "pane_id", &tmux.pane_id);
    if let Some(pane_index) = tmux.pane_index {
        details.insert("pane_index".to_string(), json!(pane_index));
    }
    insert_optional(&mut details, "pane_current_path", &tmux.pane_current_path);
    Value::Object(details)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_diagnostics_uses_client_hook_log_key() {
        let diagnostics = runtime_diagnostics(
            "/pontia/state",
            "/pontia/state/runtime.log",
            Some(("custom_hook_log", "/pontia/state/custom-hook.log")),
        );

        assert_eq!(
            diagnostics["custom_hook_log"],
            "/pontia/state/custom-hook.log"
        );
        assert!(diagnostics.get("pi_hook_log").is_none());
    }
}
