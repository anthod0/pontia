use super::{CodexService, confirmed_exits, observation_changes, string};
use crate::runtime::{
    CodexRuntime, SubscriptionState,
    protocol::{CONTROL_TIMEOUT, Connection, Notification},
};
use pontia_application::{AgentBindingService, client_contract::ClientExitOutcome};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use tokio::sync::broadcast;

impl CodexService {
    pub(crate) async fn exit(
        &self,
        target: &pontia_application::runtime::ControlTarget,
    ) -> Result<ClientExitOutcome> {
        let session = &target.session_id;
        let runtime = self.runtime(session).await?;
        let _operation = runtime.lock_session(session).await;
        target.validate(&self.pool).await?;
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session)
            .await?;
        if let Some(binding) = binding {
            self.close_thread(session, &runtime, &binding.client_session_key)
                .await?;
        } else if runtime.subscription(session).await.is_some() {
            return Err(Error::ControlUnknown(
                "unbound Codex Session still has subscription state".into(),
            ));
        } else {
            let _current = runtime.current_guard().await?;
            self.confirm_exit(session, &runtime).await;
        }
        Ok(ClientExitOutcome::Confirmed {
            reason: "thread_unloaded".into(),
        })
    }

    async fn confirm_exit(&self, session: &str, runtime: &CodexRuntime) {
        runtime
            .set_subscription(session, SubscriptionState::ExitPending)
            .await;
        confirmed_exits().lock().await.insert(session.into());
        let _ = observation_changes().send((runtime.root.clone(), session.into()));
    }

    async fn close_thread(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        thread: &str,
    ) -> Result<()> {
        let connection = runtime.connection().await?;
        let mut events = connection.events.subscribe();
        // Consume native facts here as well: the observer shares the Session operation lock.
        let stopped_revision = tokio::time::timeout(CONTROL_TIMEOUT, async {
            let (turns, revision) = self.exit_turns(session, runtime, &connection, thread).await?;
            let active = active_turn(&turns)?;
            if let Some(native) = active {
                connection
                    .call("turn/interrupt", json!({"threadId":thread,"turnId":native}))
                    .await?;
                loop {
                    let (turns, revision) = self.exit_turns(session, runtime, &connection, thread).await?;
                    if active_turn(&turns)?.is_some_and(|id| id != native) {
                        return Err(Error::ControlUnknown(
                            "Codex active turn changed while stopping; thread was not archived".into(),
                        ));
                    }
                    match turns.iter().find(|turn| turn["id"] == native) {
                        Some(turn) if terminal(turn) => return Ok(revision),
                        Some(_) => {},
                        None => return Err(Error::ControlUnknown(
                            "Codex stopping turn disappeared without a terminal result; thread was not archived".into(),
                        )),
                    }
                    wait_for_change(&mut events, thread).await?;
                }
            }
            Ok(revision)
        })
        .await
        .map_err(|_| Error::ControlUnknown(
            "Codex stop was not confirmed before the control deadline; thread was not archived".into(),
        ))??;
        runtime.current_guard().await?;

        // Archive can unload even if its receipt is lost. Persist the existing reload
        // suppression before sending it, without projecting an unload or exit fact.
        self.suspend_reload(session).await?;
        let result = self
            .archive_and_restore(
                session,
                runtime,
                &connection,
                thread,
                &mut events,
                stopped_revision,
            )
            .await;
        let _ = observation_changes().send((runtime.root.clone(), session.into()));
        let revision = result?;
        let _current = runtime.current_guard().await?;
        let _activity = connection.confirm_activity(thread, revision).await?;
        self.confirm_exit(session, runtime).await;
        Ok(())
    }

    async fn archive_and_restore(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        connection: &Connection,
        thread: &str,
        events: &mut broadcast::Receiver<Notification>,
        stopped_revision: u64,
    ) -> Result<u64> {
        connection
            .archive_thread(thread, stopped_revision)
            .await
            .map_err(|error| {
                stage_error(
                    error,
                    "Codex archive failed; archive, loading and subscription state are unconfirmed",
                )
            })?;
        tokio::time::timeout(CONTROL_TIMEOUT, async {
            loop {
                if let Some(revision) = self.thread_unloaded(runtime, connection, thread).await? {
                    let _activity = connection.confirm_activity(thread, revision).await?;
                    return Ok(());
                }
                wait_for_change(events, thread).await?;
            }
        })
        .await
        .map_err(|_| Error::ControlUnknown("Codex archive acknowledged, but thread unload was not confirmed; history has not been unarchived".into()))?
        .map_err(|error| stage_error(error, "Codex archive acknowledged, but thread unload is unconfirmed; history has not been unarchived"))?;

        runtime
            .set_subscription(session, SubscriptionState::ExitPending)
            .await;

        let restored = connection.call("thread/unarchive", json!({"threadId":thread})).await
            .map_err(|error| match error {
                Error::StateConflict(message) => Error::StateConflict(format!(
                    "Codex thread unloaded, but unarchive was rejected; history remains archived: {message}")),
                error => Error::ControlUnknown(format!(
                    "Codex thread unloaded, but unarchive result is unknown; history archive state is unknown: {error}")),
            })?;
        runtime.current_guard().await.map_err(|error| {
            stage_error(
                error,
                "Codex unarchive acknowledged, but final loading state is unknown",
            )
        })?;
        if restored["thread"]["id"] != thread || restored["thread"]["status"]["type"] != "notLoaded"
        {
            return Err(Error::ControlUnknown(
                "Codex unarchive did not confirm the original thread as unloaded; exit is incomplete".into(),
            ));
        }
        // A different frontend may have resumed or started work during archive/unarchive.
        let (turns, _) = self
            .exit_turns(session, runtime, connection, thread)
            .await
            .map_err(|error| {
                stage_error(
                    error,
                    "Codex history unarchived, but final execution state is unknown",
                )
            })?;
        if active_turn(&turns)?.is_some() {
            return Err(Error::ControlUnknown(
                "Codex history unarchived, but the thread has active work; exit is incomplete"
                    .into(),
            ));
        }
        self.thread_unloaded(runtime, connection, thread)
            .await
            .map_err(|error| {
                stage_error(
                    error,
                    "Codex history unarchived, but final loading state is unknown",
                )
            })?
            .ok_or_else(|| {
                Error::ControlUnknown(
                    "Codex history unarchived, but the thread is loaded; exit is incomplete".into(),
                )
            })
    }

    async fn exit_turns(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        connection: &Connection,
        thread: &str,
    ) -> Result<(Vec<Value>, u64)> {
        let revision = connection.activity_revision(thread).await;
        let turns = self.turns(connection, thread).await?.unwrap_or_default();
        active_turn(&turns)?;
        self.reconcile_turns(session, runtime, &turns).await?;
        Ok((turns, revision))
    }

    async fn thread_unloaded(
        &self,
        runtime: &CodexRuntime,
        connection: &Connection,
        thread: &str,
    ) -> Result<Option<u64>> {
        let revision = connection.activity_revision(thread).await;
        let snapshot = connection
            .call(
                "thread/read",
                json!({"threadId":thread,"includeTurns":false}),
            )
            .await?;
        runtime.current_guard().await?;
        if snapshot["thread"]["id"] != thread {
            return Err(Error::ControlUnknown(
                "Codex returned a different thread while checking unload".into(),
            ));
        }
        match snapshot["thread"]["status"]["type"].as_str() {
            Some("notLoaded") => Ok(Some(revision)),
            Some("idle" | "active" | "systemError") => Ok(None),
            _ => Err(Error::ControlUnknown(
                "Codex thread loading state is unknown".into(),
            )),
        }
    }
}

fn terminal(turn: &Value) -> bool {
    matches!(
        turn["status"].as_str(),
        Some("completed" | "failed" | "interrupted")
    )
}

fn active_turn(turns: &[Value]) -> Result<Option<&str>> {
    let mut active = None;
    for turn in turns {
        let id = string(turn, "id")?;
        if turn["status"] == "inProgress" && active.is_none() {
            active = Some(id);
        } else if !terminal(turn) {
            return Err(Error::ControlUnknown(
                "Codex execution state is ambiguous".into(),
            ));
        }
    }
    Ok(active)
}

async fn wait_for_change(
    events: &mut broadcast::Receiver<Notification>,
    thread: &str,
) -> Result<()> {
    loop {
        match events.recv().await {
            Ok(event) if event["method"] == "pontia/disconnected" => {
                return Err(Error::ControlUnknown(
                    "Codex disconnected during exit".into(),
                ));
            }
            Ok(event)
                if event["params"]["threadId"] == thread
                    && matches!(
                        event["method"].as_str(),
                        Some("turn/started" | "turn/completed" | "thread/status/changed")
                    ) =>
            {
                return Ok(());
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(_)) => return Ok(()),
            Err(broadcast::error::RecvError::Closed) => {
                return Err(Error::ControlUnknown(
                    "Codex event stream closed during exit".into(),
                ));
            }
        }
    }
}

fn stage_error(error: Error, context: &str) -> Error {
    match error {
        Error::StateConflict(message) => Error::StateConflict(format!("{context}: {message}")),
        error => Error::ControlUnknown(format!("{context}: {error}")),
    }
}
