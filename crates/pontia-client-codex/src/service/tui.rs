use super::{CodexService, string};
use crate::runtime::{CodexRuntime, TuiTarget};
use pontia_application::AgentBindingService;
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

impl CodexService {
    pub async fn open_tui(&self, session: &str) -> Result<()> {
        let runtime = self.runtime(session).await?;
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session)
            .await?
            .ok_or_else(|| {
                Error::StateConflict("Send the first message before opening Codex TUI".into())
            })?;
        self.open_tui_with_runtime(
            session,
            &runtime,
            &json!({"id":binding.client_session_key,"cwd":binding.launch_cwd}),
        )
        .await
    }

    pub(super) async fn open_tui_with_runtime(
        &self,
        session: &str,
        runtime: &Arc<CodexRuntime>,
        thread: &Value,
    ) -> Result<()> {
        self.profiles().configured_codex_binding(session).await?;
        // Serialize owner selection as well as launch: different Sessions can refer to one TUI.
        let _interfaces = runtime.interfaces.lock().await;
        let thread_id = string(thread, "id")?;
        let owners: Vec<String> = sqlx::query_scalar("SELECT t.owner_session_id FROM codex_tui_bindings t JOIN runtime_bindings r ON r.session_id=t.owner_session_id WHERE r.runtime_handle=? ORDER BY t.connected DESC, (t.owner_session_id=?) DESC, t.owner_session_id")
            .bind(runtime.root.to_string_lossy().as_ref()).bind(session).fetch_all(&self.pool).await?;
        let connected_owner = runtime.connected_tui_owner(thread_id).await?;
        let owner = if let Some(owner) = &connected_owner {
            owner.clone()
        } else {
            let mut owner = session.to_owned();
            for candidate in owners {
                if runtime
                    .saved_target(&candidate)?
                    .is_some_and(|target| target.thread["id"] == thread_id)
                {
                    owner = candidate;
                    break;
                }
            }
            owner
        };
        if let Some(target) = runtime.saved_target(&owner)?
            && target.thread["id"]
                .as_str()
                .is_some_and(|id| id != thread_id)
        {
            return Err(Error::StateConflict("This TUI last displayed another Codex thread; open that Session's TUI or use /resume in the existing TUI".into()));
        }
        sqlx::query("INSERT INTO codex_tui_bindings(owner_session_id,target_session_id,runtime_instance_id) VALUES(?,?,?) ON CONFLICT(owner_session_id) DO NOTHING")
            .bind(&owner).bind(session).bind(&runtime.instance_id).execute(&self.pool).await?;
        if connected_owner.is_none() {
            let _current = runtime.current_guard().await?;
            sqlx::query("UPDATE codex_tui_bindings SET connected=FALSE WHERE owner_session_id=?")
                .bind(&owner)
                .execute(&self.pool)
                .await?;
        }
        let attached = runtime
            .open_tui(&owner, thread_id, Path::new(string(thread, "cwd")?))
            .await;
        // Keep the pane available for inspection even if attachment failed or needs user input.
        if let Ok((socket, pane)) = runtime.tui_pane(&owner) {
            let _current = runtime.current_guard().await?;
            sqlx::query("UPDATE codex_tui_bindings SET tmux_socket_path=?,tmux_pane_id=?,runtime_instance_id=? WHERE owner_session_id=?")
                .bind(socket).bind(pane).bind(&runtime.instance_id).bind(&owner).execute(&self.pool).await?;
        }
        let target = attached?;
        if !self.record_tui_target(runtime, target).await? {
            return Err(Error::StateConflict(
                "TUI attachment changed while opening; retry Open TUI".into(),
            ));
        }
        Ok(())
    }
    pub(super) async fn record_tui_target(
        &self,
        runtime: &CodexRuntime,
        target: TuiTarget,
    ) -> Result<bool> {
        let _current = runtime.current_guard().await?;
        let targets = runtime.tui_targets.lock().await;
        if !targets
            .get(&target.owner_session_id)
            .is_some_and(|current| {
                current.connection_id == target.connection_id
                    && current.connected == target.connected
                    && current.thread == target.thread
            })
        {
            return Ok(false);
        }
        if !target.connected {
            sqlx::query("UPDATE codex_tui_bindings SET connected=FALSE,runtime_instance_id=?,connection_id=? WHERE owner_session_id=?").bind(&runtime.instance_id).bind(&target.connection_id).bind(&target.owner_session_id).execute(&self.pool).await?;
            return Ok(true);
        }
        let Some(thread_id) = target.thread["id"].as_str() else {
            return Ok(false);
        };
        let observation = self.observed_session(&runtime.root, runtime, &target.thread);
        let session = pontia_application::sessions::NativeSessionService::new(
            self.pool.clone(),
            self.event_ingest.clone(),
        )
        .resolve_observed_session("codex", thread_id, observation)
        .await?;
        sqlx::query("UPDATE codex_tui_bindings SET target_session_id=?,connected=TRUE,runtime_instance_id=?,connection_id=? WHERE owner_session_id=?")
            .bind(&session).bind(&runtime.instance_id).bind(&target.connection_id).bind(&target.owner_session_id).execute(&self.pool).await?;
        Ok(true)
    }
}
