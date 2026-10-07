use pontia_core::{Error, Result, time::utc_now};
use pontia_runtime::RuntimeStartResult;
use pontia_storage_sqlite::repositories::session_runtimes::SessionRuntimeRecord;

pub(crate) fn runtime_binding_record(
    session_id: &str,
    runtime: &RuntimeStartResult,
) -> Result<SessionRuntimeRecord> {
    Ok(SessionRuntimeRecord {
        runtime_id: runtime
            .runtime_id()
            .ok_or_else(|| Error::Domain("launch returned no runtime_id".into()))?
            .into(),
        session_id: session_id.into(),
        role: "tui".into(),
        state: "starting".into(),
        start_command: runtime.metadata["start_command"]
            .as_str()
            .map(str::to_owned),
        tmux_socket_path: runtime.tmux_socket_path().map(str::to_owned),
        tmux_pane_id: runtime.tmux_pane_id().map(str::to_owned),
        process_fingerprint: runtime
            .metadata
            .get("tmux_process_fingerprint")
            .filter(|value| !value.is_null())
            .map(serde_json::to_string)
            .transpose()?,
        created_at: utc_now()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|err| Error::Domain(err.to_string()))?,
    })
}
