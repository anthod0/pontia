use serde_json::{Value, json};

use pontia_agent_clients as agent_clients;
use pontia_core::error::{Error, Result};

use super::{RuntimeBindingTmuxRequest, RuntimeBindingUpsertRequest};
use crate::SessionCapabilities;

pub(super) fn validate_required(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Domain(format!("{field} is required")));
    }
    Ok(())
}

pub(super) fn is_fork_start(request: &RuntimeBindingUpsertRequest) -> bool {
    matches!(request.start_kind.as_deref().map(str::trim), Some("fork"))
}

pub(super) fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(super) fn capabilities_for_tmux(
    client_spec: &agent_clients::AgentClientSpec,
    tmux: Option<&RuntimeBindingTmuxRequest>,
) -> SessionCapabilities {
    let writable = tmux.is_some_and(|tmux| {
        non_empty(tmux.socket_path.as_deref()).is_some()
            && non_empty(tmux.pane_id.as_deref()).is_some()
    });
    let mut capabilities: SessionCapabilities = client_spec.capabilities.clone();
    capabilities.accept_task = writable;
    capabilities.interrupt = writable;
    capabilities
}

pub(super) fn binding_metadata(
    request: &RuntimeBindingUpsertRequest,
    launch_cwd: &str,
    internal_event_url: &str,
    log_dir: &str,
    runtime_log: &str,
    pi_hook_log: &str,
    capabilities: &SessionCapabilities,
) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "client_session_key".to_string(),
        json!(request.client_session_key),
    );
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
    metadata.insert("launch_cwd".to_string(), json!(launch_cwd));
    metadata.insert("workspace".to_string(), json!(launch_cwd));
    metadata.insert(
        "runtime_instance_id".to_string(),
        json!(request.runtime_instance_id),
    );
    insert_optional(&mut metadata, "start_command", &request.start_command);
    insert_optional(&mut metadata, "start_kind", &request.start_kind);
    insert_optional(
        &mut metadata,
        "parent_session_id",
        &request.parent_session_id,
    );
    insert_optional(
        &mut metadata,
        "parent_client_session_key",
        &request.parent_client_session_key,
    );
    insert_optional(
        &mut metadata,
        "forked_from_turn_id",
        &request.forked_from_turn_id,
    );
    insert_optional(
        &mut metadata,
        "forked_from_client_node_id",
        &request.forked_from_client_node_id,
    );
    metadata.insert("log_dir".to_string(), json!(log_dir));
    metadata.insert("runtime_log".to_string(), json!(runtime_log));
    metadata.insert("pi_hook_log".to_string(), json!(pi_hook_log));
    metadata.insert("internal_event_url".to_string(), json!(internal_event_url));
    metadata.insert("capabilities".to_string(), json!(capabilities));

    if let Some(tmux) = &request.tmux {
        if let Some(socket_path) = non_empty(tmux.socket_path.as_deref()) {
            metadata.insert("tmux_socket_path".to_string(), json!(socket_path));
        }
        if let Some(pane_id) = non_empty(tmux.pane_id.as_deref()) {
            metadata.insert("tmux_pane_id".to_string(), json!(pane_id));
        }
        metadata.insert("tmux".to_string(), tmux_metadata(tmux));
    }

    Value::Object(metadata)
}

pub(super) fn tmux_metadata(tmux: &RuntimeBindingTmuxRequest) -> Value {
    let mut metadata = serde_json::Map::new();
    insert_optional(&mut metadata, "session_id", &tmux.session_id);
    insert_optional(&mut metadata, "session_name", &tmux.session_name);
    insert_optional(&mut metadata, "window_id", &tmux.window_id);
    if let Some(window_index) = tmux.window_index {
        metadata.insert("window_index".to_string(), json!(window_index));
    }
    insert_optional(&mut metadata, "pane_id", &tmux.pane_id);
    if let Some(pane_index) = tmux.pane_index {
        metadata.insert("pane_index".to_string(), json!(pane_index));
    }
    insert_optional(&mut metadata, "pane_current_path", &tmux.pane_current_path);
    Value::Object(metadata)
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

pub(super) fn insert_optional(
    metadata: &mut serde_json::Map<String, Value>,
    key: &str,
    value: &Option<String>,
) {
    if let Some(value) = non_empty(value.as_deref()) {
        metadata.insert(key.to_string(), json!(value));
    }
}
