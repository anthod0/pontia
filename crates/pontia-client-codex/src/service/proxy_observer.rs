use super::{CodexService, confirmed_exits, observation_changes, string};
use crate::{
    proxy::{Observation, ObservedEvent, ThreadOperation},
    runtime::{CodexRuntime, SubscriptionState},
};
use pontia_application::{
    AgentBindingService, AppState, PontiaEvent, PontiaEventSource, PontiaEventType,
    sessions::{NativeSessionIdentity, NativeSessionService},
};
use pontia_core::{Error, Result};
use serde_json::json;
use tokio::sync::mpsc;

pub(super) async fn run(state: AppState, mut observations: mpsc::Receiver<Observation>) {
    let service = CodexService::new(state.event_ingest_service());
    while let Some(observation) = observations.recv().await {
        if let Err(error) = observe(&service, state.pontia_home(), observation).await {
            tracing::warn!(%error, "Codex proxy observation failed");
        }
    }
}

async fn observe(
    service: &CodexService,
    root: &std::path::Path,
    observation: Observation,
) -> Result<()> {
    let runtime = CodexRuntime::ensure(root).await?;
    let connection = runtime.connection().await?;
    if connection.identity != observation.daemon {
        return Err(Error::ControlUnknown(
            "Codex proxy observed a replaced daemon".into(),
        ));
    }
    match observation.event {
        ObservedEvent::ThreadOpened { operation, result } => {
            let thread = &result["thread"];
            if thread["ephemeral"] == true {
                return Ok(());
            }
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
            let session =
                NativeSessionService::new(service.pool.clone(), service.event_ingest.clone())
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
            let _operation = runtime.lock_session(&session).await;
            if service.root(&session).await? != runtime.root {
                return Err(Error::StateConflict(
                    "Codex Session belongs to another control root".into(),
                ));
            }
            if operation == ThreadOperation::Start {
                // Before the first input even thread/read can fail on an empty rollout.
                // The successful start response already confirms identity and readiness.
                service.ready(&session, &runtime, thread).await?;
                service.model_snapshot(&session, &runtime, &result).await?;
                runtime
                    .set_subscription(&session, SubscriptionState::AwaitingFirstInput)
                    .await;
                let _ = observation_changes().send((runtime.root.clone(), session));
                return Ok(());
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
                return Ok(());
            }
            {
                let _current = runtime.current_guard().await?;
                let mut unloaded = connection.unloaded_threads.lock().await;
                if unloaded
                    .get(native)
                    .is_some_and(|sequence| *sequence > boundary)
                {
                    return Ok(());
                }
                unloaded.remove(native);
            }
            let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
                .bind(&session)
                .fetch_one(&service.pool)
                .await?;
            if state == "exited" {
                service
                    .event_ingest
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
                .bind(&session).execute(&service.pool).await?;
            subscribe(service, &runtime, &session, native).await?;
        }
        ObservedEvent::TurnStarted(native) => {
            let Some(binding) = AgentBindingService::new(service.pool.clone())
                .binding_for_client_session("codex", &native)
                .await?
            else {
                return Ok(());
            };
            let session = &binding.session_id;
            let _operation = runtime.lock_session(session).await;
            if !matches!(
                runtime.subscription(session).await,
                Some(SubscriptionState::Available | SubscriptionState::ExitPending)
            ) && !service.reload_suspended(session).await?
                && !CodexService::has_confirmed_exit(session).await
                && service.session_accepts_control(session).await?
            {
                subscribe(service, &runtime, session, &native).await?;
            }
        }
    }
    Ok(())
}

async fn subscribe(
    service: &CodexService,
    runtime: &CodexRuntime,
    session: &str,
    native: &str,
) -> Result<()> {
    // Attach without overriding the settings selected by the CLI.
    let result = service
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

#[cfg(test)]
mod tests;
