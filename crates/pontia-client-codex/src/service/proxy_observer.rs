use super::CodexService;
use crate::{
    proxy::{Observation, ObservedEvent},
    runtime::{CodexRuntime, SubscriptionState},
};
use pontia_agent_control::{AgentBindingService, AgentControl};
use pontia_core::{Error, Result};

use tokio::sync::mpsc;

pub(super) async fn run(state: AgentControl, mut observations: mpsc::Receiver<Observation>) {
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
        ObservedEvent::ThreadOpened {
            runtime_id,
            operation,
            result,
            params,
            profile_binding,
        } => {
            let thread = &result["thread"];
            if thread["ephemeral"] == true {
                return Ok(());
            }
            let (session, _operation) = service
                .adopt_opened_thread(&runtime, operation, &result, &params, profile_binding)
                .await?;
            service
                .register_proxy_runtime(&session, &runtime_id)
                .await?;
        }
        ObservedEvent::ThreadUnsubscribed { thread, runtime_id } => {
            service
                .end_proxy_runtime(&runtime, &thread, &runtime_id)
                .await?;
        }
        ObservedEvent::ConnectionClosed(runtimes) => {
            for (thread, runtime_id) in runtimes {
                if let Err(error) = service
                    .end_proxy_runtime(&runtime, &thread, &runtime_id)
                    .await
                {
                    tracing::warn!(%thread, %error, "Codex proxy close observation failed");
                }
            }
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
