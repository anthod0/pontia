use super::{CodexService, confirmed_exits, observation_changes, string};
use crate::{
    proxy::ThreadOperation,
    runtime::{CodexRuntime, SubscriptionState},
};
use pontia_agent_control::{
    PontiaEvent, PontiaEventSource, PontiaEventType,
    sessions::{NativeSessionIdentity, NativeSessionService},
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};

impl CodexService {
    pub(super) async fn adopt_opened_thread(
        &self,
        runtime: &CodexRuntime,
        operation: ThreadOperation,
        result: &Value,
        params: &Value,
        session_instructions: Option<Value>,
        reservation: Option<crate::proxy::open::Reservation>,
    ) -> Result<(String, tokio::sync::OwnedMutexGuard<()>)> {
        let connection = runtime.connection().await?;
        let thread = &result["thread"];
        let native = string(thread, "id")?;
        let cwd = string(thread, "cwd")?;
        if thread["canAcceptDirectInput"] != true
            || !matches!(
                thread["status"]["type"].as_str(),
                Some("idle" | "active" | "systemError")
            )
        {
            return Err(Error::Unavailable(
                "Codex proxy thread is not loaded".into(),
            ));
        }
        let existing = pontia_agent_control::AgentBindingService::new(self.pool.clone())
            .binding_for_client_session("codex", native)
            .await?;
        let parent = self.fork_parent_session(thread, params).await?;
        for session in existing
            .as_ref()
            .map(|binding| &binding.session_id)
            .into_iter()
            .chain(parent.as_ref())
        {
            if crate::runtime::CodexServerTarget::for_session(&self.pool, session).await?
                != runtime.target
            {
                return Err(Error::StateConflict(
                    "Codex thread belongs to another app-server target".into(),
                ));
            }
        }
        let expected_session = reservation
            .as_ref()
            .map(|reservation| reservation.session.clone())
            .or_else(|| existing.as_ref().map(|binding| binding.session_id.clone()))
            .unwrap_or_else(|| pontia_core::ids::new_session_id().to_string());
        // Fence recovery before publishing a new identity. Existing Sessions retain
        // identity-before-operation lock ordering used by resume commands.
        let mut new_session_guard = if existing.is_none() {
            Some(runtime.lock_session(&expected_session).await)
        } else {
            None
        };
        let session = NativeSessionService::new(self.pool.clone(), self.event_ingest.clone())
            .resolve_subscription_session(
                "codex",
                native,
                NativeSessionIdentity {
                    launch_cwd: cwd.into(),
                    client_session_file: thread["path"].as_str().map(str::to_owned),
                },
                pontia_agent_control::sessions::NativeSubscription {
                    metadata: json!({"codex_control_root":runtime.root,"codex_launch_cwd":cwd}),
                    session_instructions,
                    launch_mode: runtime.target.launch_mode(),
                    ..Default::default()
                },
                Some(&expected_session),
            )
            .await?;
        let closed = reservation
            .as_ref()
            .map(|reservation| reservation.close.clone());
        if let Some(reservation) = reservation {
            if reservation.session != session {
                return Err(Error::StateConflict(
                    "Codex Session identity changed during proxy preparation".into(),
                ));
            }
            let _ = reservation.bound.send(());
        }
        if session != expected_session {
            new_session_guard = None;
        }
        let operation_guard = match new_session_guard {
            Some(guard) => guard,
            None => runtime.lock_session(&session).await,
        };
        if closed.is_some_and(|closed| *closed.borrow()) {
            return Err(Error::StateConflict(
                "Codex TUI connection was closed by Session exit".into(),
            ));
        }
        if self.root(&session).await? != runtime.root
            || crate::runtime::CodexServerTarget::for_session(&self.pool, &session).await?
                != runtime.target
        {
            return Err(Error::StateConflict(
                "Codex Session belongs to another control root or app-server target".into(),
            ));
        }
        if let Some(parent) = parent {
            self.record_fork_lineage(&session, &parent, params).await?;
        }
        if operation == ThreadOperation::Start {
            if connection
                .pending_archives
                .lock()
                .await
                .contains_key(native)
                || connection
                    .unloaded_threads
                    .lock()
                    .await
                    .contains_key(native)
            {
                return Err(Error::Unavailable(
                    "Codex thread was unloaded before attachment".into(),
                ));
            }
            // Before the first input even thread/read can fail on an empty rollout.
            // The successful start response already confirms identity and readiness.
            self.ready(&session, runtime, thread).await?;
            self.model_snapshot(&session, runtime, result).await?;
            runtime
                .set_subscription(&session, SubscriptionState::AwaitingFirstInput)
                .await;
            let _ =
                observation_changes().send((runtime.root.clone(), runtime.target, session.clone()));
            return Ok((session, operation_guard));
        }
        let boundary = self.confirm_loaded_thread(runtime, native).await?;
        self.reconcile_proxy_archive(runtime, &session, native, Some(boundary))
            .await?;
        let state = self
            .queries()
            .session_state(&session)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        if state == pontia_core::domain::SessionState::Exited {
            self.event_ingest
                .ingest_pontia_event(PontiaEvent::new(
                    &session,
                    None,
                    PontiaEventSource::RuntimeManager,
                    "codex",
                    PontiaEventType::SessionResuming,
                    json!({}),
                ))
                .await?;
        }
        confirmed_exits().lock().await.remove(&session);
        self.clear_reload_suspended(&session).await?;
        self.subscribe_opened_thread(runtime, &session, native)
            .await?;
        Ok((session, operation_guard))
    }

    pub(super) async fn confirm_loaded_thread(
        &self,
        runtime: &CodexRuntime,
        native: &str,
    ) -> Result<u64> {
        let connection = runtime.connection().await?;
        // A queued proxy receipt must not resurrect a thread unloaded since the operation.
        let (snapshot, boundary) = connection
            .call_observed(
                "thread/read",
                json!({"threadId":native,"includeTurns":false}),
            )
            .await?;
        if snapshot["thread"]["id"] != native
            || snapshot["thread"]["canAcceptDirectInput"] != true
            || !matches!(
                snapshot["thread"]["status"]["type"].as_str(),
                Some("idle" | "active" | "systemError")
            )
        {
            return Err(Error::Unavailable(
                "Codex thread was unloaded before attachment".into(),
            ));
        }
        {
            let _current = runtime.current_guard().await?;
            let mut unloaded = connection.unloaded_threads.lock().await;
            if unloaded
                .get(native)
                .is_some_and(|sequence| *sequence > boundary)
            {
                return Err(Error::Unavailable(
                    "Codex thread was unloaded before attachment".into(),
                ));
            }
            unloaded.remove(native);
        }
        Ok(boundary)
    }

    pub(super) async fn subscribe_opened_thread(
        &self,
        runtime: &CodexRuntime,
        session: &str,
        native: &str,
    ) -> Result<()> {
        // Attach without overriding the settings selected by the CLI.
        let result = self
            .subscribe_with_params(
                session,
                runtime,
                native,
                false,
                json!({"threadId":native,"excludeTurns":true}),
            )
            .await;
        let _ = observation_changes().send((runtime.root.clone(), runtime.target, session.into()));
        result
    }
}
