use super::{CodexService, confirmed_exits, observation_changes, string};
use crate::{
    proxy::ThreadOperation,
    runtime::{CodexRuntime, SubscriptionState},
};
use pontia_application::{
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
    ) -> Result<String> {
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
            return Err(Error::CapabilityUnavailable(
                "Codex proxy thread is not loaded".into(),
            ));
        }
        let session = NativeSessionService::new(self.pool.clone(), self.event_ingest.clone())
            .resolve_subscription_session(
                "codex",
                native,
                NativeSessionIdentity {
                    launch_cwd: cwd.into(),
                    client_session_file: thread["path"].as_str().map(str::to_owned),
                },
                json!({"codex_control_root":runtime.root,"codex_launch_cwd":cwd}),
            )
            .await?;
        self.record_fork_lineage(&session, thread, params).await?;
        let _operation = runtime.lock_session(&session).await;
        if self.root(&session).await? != runtime.root {
            return Err(Error::StateConflict(
                "Codex Session belongs to another control root".into(),
            ));
        }
        if operation == ThreadOperation::Start {
            // Before the first input even thread/read can fail on an empty rollout.
            // The successful start response already confirms identity and readiness.
            self.ready(&session, runtime, thread).await?;
            self.model_snapshot(&session, runtime, result).await?;
            runtime
                .set_subscription(&session, SubscriptionState::AwaitingFirstInput)
                .await;
            let _ = observation_changes().send((runtime.root.clone(), session.clone()));
            return Ok(session);
        }
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
            return Err(Error::CapabilityUnavailable(
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
                return Err(Error::CapabilityUnavailable(
                    "Codex thread was unloaded before attachment".into(),
                ));
            }
            unloaded.remove(native);
        }
        let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
            .bind(&session)
            .fetch_one(&self.pool)
            .await?;
        if state == "exited" {
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
        sqlx::query("UPDATE sessions SET metadata=json_remove(metadata,'$.codex_observation_unloaded') WHERE session_id=?")
                .bind(&session).execute(&self.pool).await?;
        self.subscribe_opened_thread(runtime, &session, native)
            .await?;
        Ok(session)
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
        let _ = observation_changes().send((runtime.root.clone(), session.into()));
        result
    }
}
