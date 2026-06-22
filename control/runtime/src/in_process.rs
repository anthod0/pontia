use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use serde_json::json;
use time::format_description::well_known::Rfc3339;

use pontia_agent_clients::AgentClientCapabilities;
use pontia_core::{
    error::{Error, Result},
    ids::new_runtime_instance_id,
    time::utc_now,
};

use super::{
    RuntimeStartRequest, RuntimeStartResult,
    utils::{sanitize_identifier, short_session_id},
};

#[derive(Debug, Clone)]
struct InProcessRuntimeState {
    alive: bool,
}

pub(super) fn start_session(
    request: RuntimeStartRequest,
    capabilities: AgentClientCapabilities,
    restart_count: i64,
) -> Result<RuntimeStartResult> {
    let runtime_instance_id = new_runtime_instance_id().to_string();
    let started_at = utc_now()
        .format(&Rfc3339)
        .map_err(|err| Error::Domain(format!("invalid runtime timestamp: {err}")))?;
    let runtime_dir = std::env::temp_dir()
        .join("pontia-test-runtimes")
        .join(&request.session_id);
    std::fs::create_dir_all(&runtime_dir)?;
    let log_path = runtime_dir.join("runtime.log");
    let adapter_event_log = runtime_dir.join("adapter-events.jsonl");
    std::fs::File::create(&log_path)?;
    let runtime_dir = runtime_dir.display().to_string();
    let log_path = log_path.display().to_string();
    let adapter_event_log = adapter_event_log.display().to_string();
    let runtime_handle = runtime_handle(&request);
    registry()
        .lock()
        .expect("in-process runtime registry lock")
        .insert(
            runtime_handle.clone(),
            InProcessRuntimeState { alive: true },
        );
    Ok(RuntimeStartResult {
        runtime_kind: "in_process".to_string(),
        runtime_handle: runtime_handle.clone(),
        capabilities,
        metadata: json!({
            "backend": "in_process",
            "in_process_runtime": true,
            "in_process": {
                "runtime_handle": runtime_handle,
            },
            "runtime_dir": runtime_dir,
            "runtime_log": log_path,
            "log_path": log_path,
            "adapter_event_log": adapter_event_log,
            "launch_cwd": request.workspace,
            "internal_event_url": "in-process://internal-events",
            "handle": request.handle,
            "role": request.role,
            "started_at": started_at,
            "restart_count": restart_count,
            "runtime_instance_id": runtime_instance_id,
        }),
    })
}

pub(super) fn terminate_session(runtime_handle: &str) -> bool {
    if let Some(runtime) = registry()
        .lock()
        .expect("in-process runtime registry lock")
        .get_mut(runtime_handle)
    {
        runtime.alive = false;
        return true;
    }
    false
}

pub(super) fn is_alive(runtime_handle: &str) -> Option<bool> {
    registry()
        .lock()
        .expect("in-process runtime registry lock")
        .get(runtime_handle)
        .map(|runtime| runtime.alive)
}

pub(super) fn reset_registry() {
    registry()
        .lock()
        .expect("in-process runtime registry lock")
        .clear();
}

fn registry() -> &'static Mutex<HashMap<String, InProcessRuntimeState>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, InProcessRuntimeState>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_handle(request: &RuntimeStartRequest) -> String {
    let handle = request
        .handle
        .as_deref()
        .map(|value| value.trim_start_matches('@'))
        .filter(|value| !value.is_empty())
        .map(sanitize_identifier);
    let role = request
        .role
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(sanitize_identifier);
    let Some(handle) = handle else {
        return format!("{}:{}", request.client_type, request.session_id);
    };
    let Some(role) = role else {
        return format!(
            "{}:{}:{}",
            request.client_type,
            handle,
            short_session_id(&request.session_id)
        );
    };
    format!(
        "{}:{}:{}:{}",
        request.client_type,
        handle,
        role,
        short_session_id(&request.session_id)
    )
}
