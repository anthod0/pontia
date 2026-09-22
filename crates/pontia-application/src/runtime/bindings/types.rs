use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBindingUpsertRequest {
    pub session_id: Option<String>,
    pub runtime_instance_id: Option<String>,
    pub client_type: String,
    pub client_session_key: String,
    pub client_session_file: Option<String>,
    pub client_session_dir: Option<String>,
    pub client_cwd: Option<String>,
    pub launch_cwd: Option<String>,
    pub start_command: Option<String>,
    pub start_kind: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_client_session_key: Option<String>,
    pub forked_from_turn_id: Option<String>,
    pub forked_from_client_node_id: Option<String>,
    #[serde(default)]
    pub lineage_metadata: Value,
    pub tmux: Option<RuntimeBindingTmuxRequest>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RuntimeBindingTmuxRequest {
    pub socket_path: Option<String>,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub window_id: Option<String>,
    pub window_index: Option<i64>,
    pub pane_id: Option<String>,
    pub pane_index: Option<i64>,
    pub pane_current_path: Option<String>,
}
