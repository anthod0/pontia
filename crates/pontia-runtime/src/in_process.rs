use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
};

use serde_json::json;
use time::format_description::well_known::Rfc3339;

use pontia_core::client_capabilities::AgentClientCapabilities;
use pontia_core::{
    error::{Error, Result},
    ids::new_runtime_id,
    time::utc_now,
};

use super::{RuntimeStartRequest, RuntimeStartResult, paths};

#[derive(Debug, Clone)]
struct InProcessRuntimeState {
    alive: bool,
}

pub(super) fn start_session(
    pontia_home: &Path,
    request: RuntimeStartRequest,
    capabilities: AgentClientCapabilities,
    restart_count: i64,
) -> Result<RuntimeStartResult> {
    let runtime_id = request
        .runtime_id
        .clone()
        .unwrap_or_else(|| new_runtime_id().to_string());
    let started_at = utc_now()
        .format(&Rfc3339)
        .map_err(|err| Error::Domain(format!("invalid runtime timestamp: {err}")))?;
    let log_paths = paths::log_paths(pontia_home);
    let log_dir = log_paths.log_dir.display().to_string();
    let log_path = log_paths.runtime_log.display().to_string();
    let runtime_handle = runtime_id.clone();
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
            "log_dir": log_dir,
            "runtime_log": log_path,
            "log_path": log_path,
            "launch_cwd": request.workspace,
            "handle": request.handle,
            "role": request.role,
            "started_at": started_at,
            "restart_count": restart_count,
            "runtime_id": runtime_id,
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
