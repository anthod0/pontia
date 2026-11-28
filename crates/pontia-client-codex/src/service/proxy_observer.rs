use super::CodexService;
use crate::{
    proxy::{Observation, ObservedEvent},
    runtime::{CodexRuntime, SubscriptionState},
};
use pontia_agent_control::ClientServiceContext;
use pontia_core::{Error, Result};

use tokio::sync::mpsc;

pub(super) async fn run(
    state: ClientServiceContext,
    mut observations: mpsc::Receiver<Observation>,
) {
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
    let runtime = if matches!(observation.event, ObservedEvent::ConnectionClosed(_)) {
        // A peer departure remains conclusive even if the upstream daemon was lost.
        let Some(runtime) = CodexRuntime::existing(&root.canonicalize()?).await else {
            return Ok(());
        };
        runtime
    } else {
        let runtime = CodexRuntime::ensure(root, crate::runtime::CodexServerTarget::User).await?;
        if runtime.connection().await?.identity != observation.daemon {
            return Err(Error::ControlUnknown(
                "Codex proxy observed a replaced daemon".into(),
            ));
        }
        runtime
    };
    match observation.event {
        ObservedEvent::ConnectionClosed(runtimes) => {
            for (thread, runtime_id) in runtimes {
                if let Err(error) = service
                    .end_proxy_runtime(&runtime, &thread, &runtime_id, &observation.daemon)
                    .await
                {
                    tracing::warn!(%thread, %error, "Codex proxy close observation failed");
                }
            }
        }
        ObservedEvent::ThreadOpened {
            peer,
            runtime_id,
            operation,
            result,
            params,
            session_instructions,
            reservation,
        } => {
            let thread = &result["thread"];
            if thread["ephemeral"] == true {
                return Ok(());
            }
            let (session, _operation) = service
                .adopt_opened_thread(
                    &runtime,
                    operation,
                    &result,
                    &params,
                    session_instructions,
                    reservation,
                )
                .await?;
            service
                .register_proxy_runtime(&session, &runtime_id, &peer)
                .await?;
        }
        ObservedEvent::ThreadUnsubscribed { thread, runtime_id } => {
            service
                .end_proxy_runtime(&runtime, &thread, &runtime_id, &observation.daemon)
                .await?;
        }
        ObservedEvent::TurnStarted(native) => {
            let Some(binding) = service
                .store
                .bindings()
                .binding_for_client_session("codex", &native)
                .await?
            else {
                return Ok(());
            };
            let session = &binding.session_id;
            if crate::runtime::CodexServerTarget::for_session(&service.store, session).await?
                != runtime.target
            {
                return Ok(());
            }
            let _operation = runtime.lock_session(session).await;
            let subscription = runtime.subscription(session).await;
            use pontia_core::domain::SessionState;
            let accepts_turn = matches!(
                service
                    .queries()
                    .session_state(session)
                    .await?
                    .ok_or_else(Error::missing_record)?,
                SessionState::Idle
                    | SessionState::Busy
                    | SessionState::WaitingForInteraction
                    | SessionState::Interrupted
            );
            if subscription == Some(SubscriptionState::ExitPending)
                || CodexService::has_confirmed_exit(session).await
                || !accepts_turn
            {
                return Ok(());
            }
            let connection = runtime.connection().await?;
            let suspended = service.reload_suspended(session).await?
                || connection
                    .unloaded_threads
                    .lock()
                    .await
                    .contains_key(&native);
            if suspended {
                let boundary = service.confirm_loaded_thread(&runtime, &native).await?;
                service
                    .reconcile_proxy_archive(&runtime, session, &native, Some(boundary))
                    .await?;
                if CodexService::has_confirmed_exit(session).await {
                    return Ok(());
                }
                service.clear_reload_suspended(session).await?;
            }
            if suspended || subscription != Some(SubscriptionState::Available) {
                service
                    .subscribe_opened_thread(&runtime, session, &native)
                    .await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
