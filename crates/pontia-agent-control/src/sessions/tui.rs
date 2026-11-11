use std::time::Duration;

use pontia_core::{Error, Result, ids::new_runtime_id};
use pontia_runtime::{GenericRuntimeManager, ProcessObservation, TmuxProcessFingerprint};
use pontia_storage_sqlite::repositories::{
    session_runtimes::{SessionRuntimeRecord, SqliteSessionRuntimeRepository},
    sessions::SqliteSessionRepository,
};
use serde_json::json;

use super::SessionCommandService;
use crate::{ControlCommandOutcome, PontiaEvent, PontiaEventSource, PontiaEventType};

const INTERFACE_ROLE: &str = "interface";
const TUI_EXIT_TIMEOUT: Duration = Duration::from_secs(5);

impl SessionCommandService {
    pub async fn start_tui(&self, session_id: &str) -> Result<ControlCommandOutcome> {
        let _launch_guard = self.tui_gate.lock().await;
        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if !self
            .clients
            .for_client(&session.client_type)?
            .spec
            .adapter
            .lifecycle
            .independent_interface
        {
            return Err(Error::CapabilityUnavailable(
                "Session does not support an independent TUI".into(),
            ));
        }

        let repository = SqliteSessionRuntimeRepository::new(self.pool.clone());
        let existing = repository.interface_runtime(session_id).await?;
        if existing
            .as_ref()
            .is_some_and(|runtime| matches!(runtime.state.as_str(), "starting" | "running"))
        {
            return Err(Error::StateConflict(format!(
                "session {session_id} already has an active managed TUI"
            )));
        }
        let runtime_id = existing
            .map(|runtime| runtime.runtime_id)
            .unwrap_or_else(|| new_runtime_id().to_string());
        let runtime = self
            .clients
            .for_client(&session.client_type)?
            .open_interface(&self.pontia_home, session_id, &runtime_id)
            .await?;
        if runtime.runtime_id() != Some(runtime_id.as_str()) {
            crate::clients::discard_unbound_runtime(&runtime);
            return Err(Error::StateConflict(
                "TUI launch returned a different runtime identity".into(),
            ));
        }

        let binding = match interface_process_binding(session_id, &runtime) {
            Ok(binding) => binding,
            Err(error) => {
                crate::clients::discard_unbound_runtime(&runtime);
                return Err(error);
            }
        };
        let starting = self
            .event_ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id,
                None,
                PontiaEventSource::RuntimeManager,
                &session.client_type,
                PontiaEventType::RuntimeStarting,
                json!({
                    "runtime_id": runtime_id,
                    "role": INTERFACE_ROLE,
                    "start_command": binding.start_command,
                    "tmux_socket_path": binding.tmux_socket_path,
                    "tmux_pane_id": binding.tmux_pane_id,
                    "process_fingerprint": binding.process_fingerprint,
                }),
            ))
            .await;
        if let Err(error) = starting {
            crate::clients::discard_unbound_runtime(&runtime);
            return Err(error);
        }

        if let Err(error) = self
            .event_ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id,
                None,
                PontiaEventSource::RuntimeManager,
                &session.client_type,
                PontiaEventType::RuntimeReady,
                json!({"runtime_id": runtime_id, "role": INTERFACE_ROLE}),
            ))
            .await
        {
            crate::clients::discard_unbound_runtime(&runtime);
            return Err(error);
        }

        Ok(ControlCommandOutcome {
            data: json!({"runtime": repository.get(&runtime_id).await?}),
            duplicate: false,
        })
    }

    pub async fn stop_tui(&self, session_id: &str) -> Result<ControlCommandOutcome> {
        let _control_guard = self.tui_gate.lock().await;
        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if !self
            .clients
            .for_client(&session.client_type)?
            .spec
            .adapter
            .lifecycle
            .independent_interface
        {
            return Err(Error::CapabilityUnavailable(
                "Session does not support an independent TUI".into(),
            ));
        }

        let repository = SqliteSessionRuntimeRepository::new(self.pool.clone());
        let runtime = repository
            .interface_runtime(session_id)
            .await?
            .ok_or_else(|| Error::StateConflict("Session has no managed TUI".into()))?;
        if runtime.state == "exited" {
            return Err(Error::StateConflict("Managed TUI is not running".into()));
        }
        let fingerprint_json = runtime.process_fingerprint.as_deref().ok_or_else(|| {
            Error::ControlUnknown("Managed TUI has no process fingerprint".into())
        })?;
        let fingerprint: TmuxProcessFingerprint = serde_json::from_str(fingerprint_json)
            .map_err(|_| Error::ControlUnknown("Managed TUI fingerprint is invalid".into()))?;

        if GenericRuntimeManager.terminate_tmux_process(&fingerprint)? != ProcessObservation::Exited
        {
            tokio::time::timeout(TUI_EXIT_TIMEOUT, async {
                loop {
                    if GenericRuntimeManager.observe_tmux_process_fingerprint(&fingerprint)
                        == ProcessObservation::Exited
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .map_err(|_| Error::ControlUnknown("Managed TUI did not exit".into()))?;
        }

        self.record_tui_exited(
            session_id,
            &session.client_type,
            &runtime.runtime_id,
            runtime.process_fingerprint.as_deref(),
            "api_stop",
        )
        .await?;
        Ok(ControlCommandOutcome {
            data: json!({"runtime": repository.get(&runtime.runtime_id).await?}),
            duplicate: false,
        })
    }

    async fn record_tui_exited(
        &self,
        session_id: &str,
        client_type: &str,
        runtime_id: &str,
        process_fingerprint: Option<&str>,
        reason: &str,
    ) -> Result<()> {
        self.event_ingest
            .ingest_runtime_observation_event(PontiaEvent::new(
                session_id,
                None,
                PontiaEventSource::RuntimeManager,
                client_type,
                PontiaEventType::RuntimeExited,
                json!({
                    "runtime_id": runtime_id,
                    "role": INTERFACE_ROLE,
                    "reason": reason,
                    "process_fingerprint": process_fingerprint,
                }),
            ))
            .await?;
        Ok(())
    }
}

fn interface_process_binding(
    session_id: &str,
    runtime: &pontia_runtime::RuntimeStartResult,
) -> Result<SessionRuntimeRecord> {
    let runtime_id = runtime
        .runtime_id()
        .ok_or_else(|| Error::Domain("TUI launch returned no runtime_id".into()))?;
    let socket = runtime
        .tmux_socket_path()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::Domain("TUI launch returned no tmux socket".into()))?;
    let pane = runtime
        .tmux_pane_id()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::Domain("TUI launch returned no tmux pane".into()))?;
    let fingerprint = runtime
        .metadata
        .get("tmux_process_fingerprint")
        .filter(|value| !value.is_null())
        .ok_or_else(|| Error::ControlUnknown("TUI process was not verified".into()))?;
    Ok(SessionRuntimeRecord {
        runtime_id: runtime_id.into(),
        session_id: session_id.into(),
        role: INTERFACE_ROLE.into(),
        state: "starting".into(),
        start_command: runtime.metadata["start_command"]
            .as_str()
            .map(str::to_owned),
        tmux_socket_path: Some(socket.into()),
        tmux_pane_id: Some(pane.into()),
        process_fingerprint: Some(serde_json::to_string(fingerprint)?),
        created_at: String::new(),
    })
}
