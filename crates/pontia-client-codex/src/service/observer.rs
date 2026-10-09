use super::CodexService;
use crate::runtime::{CodexRuntime, SubscriptionState};
use pontia_application::AgentBindingService;
use pontia_core::{Error, Result};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{broadcast, watch};

pub struct CodexObserver {
    service: CodexService,
    root: PathBuf,
}

impl CodexObserver {
    pub fn new(event_ingest: pontia_application::EventIngestService, root: PathBuf) -> Self {
        Self {
            service: CodexService::new(event_ingest),
            root,
        }
    }

    pub async fn prepare(&self) -> Result<()> {
        let root = self.root.canonicalize()?;
        sqlx::query("UPDATE sessions SET metadata=json_set(CASE WHEN json_type(metadata)='object' THEN metadata ELSE '{}' END,'$.codex_control_root',?) WHERE client_type='codex' AND json_extract(metadata,'$.codex_control_root') IS NULL")
            .bind(root.display().to_string())
            .execute(&self.service.pool)
            .await?;
        sqlx::query("UPDATE sessions SET metadata=json_set(metadata,'$.codex_launch_cwd',COALESCE(workspace_ref,(SELECT launch_cwd FROM agent_bindings WHERE agent_bindings.session_id=sessions.session_id),(SELECT json_extract(payload,'$.workspace') FROM events WHERE events.session_id=sessions.session_id AND event_type='session.created' LIMIT 1))) WHERE client_type='codex' AND json_extract(metadata,'$.codex_launch_cwd') IS NULL AND COALESCE(workspace_ref,(SELECT launch_cwd FROM agent_bindings WHERE agent_bindings.session_id=sessions.session_id),(SELECT json_extract(payload,'$.workspace') FROM events WHERE events.session_id=sessions.session_id AND event_type='session.created' LIMIT 1)) IS NOT NULL")
            .execute(&self.service.pool)
            .await?;
        Ok(())
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            if *shutdown.borrow() {
                break;
            }
            let active: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sessions WHERE client_type='codex' AND state<>'exited'",
            )
            .fetch_one(&self.service.pool)
            .await
            .unwrap_or(0);
            if active == 0 {
                tokio::select! {
                    _ = shutdown.changed() => break,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => continue,
                }
            }
            match CodexRuntime::ensure(&self.root).await {
                Ok(runtime) => {
                    if let Err(error) = self.observe(runtime.clone(), &mut shutdown).await {
                        tracing::warn!(%error, "Codex observation interrupted; subscriptions will be reconciled after reconnect");
                    }
                    runtime.clear_subscriptions().await;
                }
                Err(error) => tracing::warn!(%error, "Codex app-server unavailable"),
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    }

    async fn observe(
        &self,
        runtime: Arc<CodexRuntime>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        runtime
            .profile_service
            .get_or_init(|| self.service.profiles());
        let connection = runtime.connection().await?;
        let mut events = connection.events.subscribe();
        let mut threads = HashMap::<String, String>::new();
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! {
                _ = shutdown.changed() => return Ok(()),
                _ = interval.tick() => {
                    let unbound: Vec<String> = sqlx::query_scalar("SELECT s.session_id FROM sessions s LEFT JOIN agent_bindings a USING(session_id) WHERE s.client_type='codex' AND s.state<>'exited' AND a.session_id IS NULL")
                        .fetch_all(&self.service.pool).await?;
                    for session in unbound {
                        self.service.event_ingest.control_available(&session);
                    }
                    let bindings: Vec<(String,String)> = sqlx::query_as("SELECT a.session_id,a.client_session_key FROM agent_bindings a JOIN sessions s USING(session_id) WHERE a.client_type='codex' AND s.state<>'exited'")
                        .fetch_all(&self.service.pool).await?;
                    for (session, thread) in bindings {
                        threads.insert(thread.clone(), session.clone());
                        if CodexService::has_confirmed_unsubscribe(&session).await { continue; }
                        if matches!(runtime.subscription(&session).await, Some(SubscriptionState::Available | SubscriptionState::ExitPending)) { continue; }
                        let _operation = runtime.lock_session(&session).await;
                        if let Err(error) = self.service.subscribe(&session, &runtime, &thread).await {
                            runtime.clear_subscription(&session).await;
                            if !connection.is_connected() { return Err(error); }
                            tracing::warn!(%session, %error, "Codex thread reconciliation failed");
                        }
                    }
                    let exited: Vec<String> = sqlx::query_scalar("SELECT session_id FROM sessions WHERE client_type='codex' AND state='exited'")
                        .fetch_all(&self.service.pool).await?;
                    for session in exited {
                        runtime.clear_subscription(&session).await;
                        CodexService::clear_confirmed_unsubscribe(&session).await;
                    }
                }
                event = events.recv() => {
                    let event = match event {
                        Ok(event) => event,
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            runtime.clear_subscriptions().await;
                            continue;
                        }
                        Err(_) => return Err(Error::CapabilityUnavailable("Codex event stream closed".into())),
                    };
                    if event["method"] == "pontia/disconnected" { return Err(Error::CapabilityUnavailable("Codex disconnected".into())); }
                    let Some(thread) = event.pointer("/params/threadId").and_then(Value::as_str) else { continue };
                    let session = match threads.get(thread) {
                        Some(session) => session.clone(),
                        None => match AgentBindingService::new(self.service.pool.clone()).binding_for_client_session("codex", thread).await? {
                            Some(binding) => binding.session_id,
                            None => continue,
                        },
                    };
                    if !matches!(runtime.subscription(&session).await, Some(SubscriptionState::Available | SubscriptionState::AwaitingFirstInput | SubscriptionState::Reconciling)) { continue; }
                    match event["method"].as_str() {
                        Some("thread/settings/updated") => {
                            let _current = runtime.current_guard().await?;
                            self.service.model_fact(&session, &event["params"]["threadSettings"]).await?;
                        }
                        Some("turn/started") => {
                            if let Some(turns) = self.service.turns(&connection, thread).await? {
                                self.service.reconcile_turns(&session, &runtime, &turns).await?;
                                runtime.set_subscription(&session, SubscriptionState::Available).await;
                            }
                        }
                        Some("turn/completed") => {
                            let _current = runtime.current_guard().await?;
                            self.service.turn_fact(&session, &event["params"]["turn"], "notification").await?;
                        }
                        Some("thread/status/changed") if event.pointer("/params/status/type").and_then(Value::as_str) == Some("notLoaded") => {
                            runtime.clear_subscription(&session).await;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
