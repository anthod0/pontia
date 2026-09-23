use super::CodexService;
use crate::runtime::{CodexRuntime, TuiTarget};
use pontia_application::{AgentBindingService, EventIngestService};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{broadcast, watch};

pub struct CodexObserver {
    service: CodexService,
    root: PathBuf,
}

impl CodexObserver {
    pub fn new(event_ingest: EventIngestService, root: PathBuf) -> Self {
        Self {
            service: CodexService::new(event_ingest),
            root,
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
            let count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM agent_bindings WHERE client_type='codex'",
            )
            .fetch_one(&self.service.pool)
            .await
            .unwrap_or(0);
            if count == 0 {
                continue;
            }
            match CodexRuntime::ensure(&self.root).await {
                Ok(runtime) => {
                    if let Err(error) = self.observe(runtime.clone(), &mut shutdown).await {
                        tracing::warn!(%error,"Codex observation interrupted; will reconcile before accepting input");
                    }
                    let _ = sqlx::query("UPDATE runtime_bindings SET adapter_details=json_set(adapter_details,'$.codex.connection','unavailable') WHERE runtime_kind='codex_app_server' AND runtime_instance_id=?").bind(&runtime.instance_id).execute(&self.service.pool).await;
                    let _ = sqlx::query(
                        "UPDATE codex_tui_bindings SET connected=FALSE WHERE runtime_instance_id=?",
                    )
                    .bind(&runtime.instance_id)
                    .execute(&self.service.pool)
                    .await;
                }
                Err(error) => tracing::warn!(%error,"Codex runtime unavailable"),
            }
            if *shutdown.borrow() {
                break;
            }
        }
        CodexRuntime::shutdown(&self.root).await;
    }

    async fn observe(
        &self,
        runtime: Arc<CodexRuntime>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        let connection = runtime.connection().await?;
        let mut events = connection.events.subscribe();
        let mut targets = runtime.targets.subscribe();
        let owners: Vec<String> =
            sqlx::query_scalar("SELECT owner_session_id FROM codex_tui_bindings")
                .fetch_all(&self.service.pool)
                .await?;
        for owner in owners {
            runtime.ensure_tui_gateway(&owner).await?;
        }
        let mut subscribed = HashSet::new();
        let mut threads = HashMap::new();
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! {
                _ = shutdown.changed() => return Ok(()),
                _ = interval.tick() => {
                    let known_targets: Vec<_> = runtime.tui_targets.lock().await.values().cloned().collect();
                    for target in known_targets { self.target(&runtime,target).await?; }
                    let bindings: Vec<(String,String)> = sqlx::query_as("SELECT session_id,client_session_key FROM agent_bindings WHERE client_type='codex'").fetch_all(&self.service.pool).await?;
                    for (session,thread) in bindings {
                        let _operation = runtime.lock_session(&session).await;
                        threads.insert(thread.clone(),session.clone());
                        let result: Result<()> = async {
                        let resumed_thread = if !subscribed.contains(&thread) {
                            let metadata = connection.call("thread/read",json!({"threadId":thread,"includeTurns":false})).await?;
                            self.service.bind(&session,&runtime,&metadata["thread"]).await?;
                            if self.service.check_archived(&session,&runtime,&thread).await? { return Ok(()); }
                            let resumed = connection.call("thread/resume",json!({"threadId":thread,"excludeTurns":true})).await?;
                            self.service.bind(&session,&runtime,&resumed["thread"]).await?;
                            self.service.model_fact(&session,&runtime.instance_id,&resumed).await?;
                            Some(resumed["thread"].clone())
                        } else { None };
                        let native = self.service.turns(&connection,&thread).await?;
                        self.service.reconcile_turns(&session,&runtime,&native).await?;
                        if let Some(resumed) = resumed_thread {
                            self.service.ready(&session,&runtime,&resumed).await?;
                            subscribed.insert(thread.clone());
                        }
                        self.service.connection_state(&session,&runtime.instance_id,"available").await?;
                        self.service.event_ingest.control_available(&session);
                            Ok(())
                        }.await;
                        if let Err(error) = result {
                            self.service.connection_state(&session,&runtime.instance_id,"unavailable").await?;
                            subscribed.remove(&thread);
                            if !connection.is_connected() { return Err(error); }
                            tracing::warn!(%session, %error, "Codex thread reconciliation failed");
                        }
                    }
                }
                event = events.recv() => {
                    let event = match event { Ok(event) => event, Err(broadcast::error::RecvError::Lagged(_)) => {
                        sqlx::query("UPDATE runtime_bindings SET adapter_details=json_set(adapter_details,'$.codex.connection','reconciling') WHERE runtime_kind='codex_app_server' AND runtime_instance_id=?").bind(&runtime.instance_id).execute(&self.service.pool).await?;
                        subscribed.clear(); continue;
                    }, Err(_) => return Err(Error::CapabilityUnavailable("Codex event stream closed".into())) };
                    if event["method"] == "pontia/disconnected" { return Err(Error::CapabilityUnavailable("Codex disconnected".into())); }
                    let Some(thread) = event.pointer("/params/threadId").and_then(Value::as_str) else { continue };
                    let session = match threads.get(thread) {
                        Some(session) => session.clone(),
                        None => match AgentBindingService::new(self.service.pool.clone()).binding_for_client_session("codex",thread).await? { Some(binding) => binding.session_id, None => continue },
                    };
                    match event["method"].as_str() {
                        Some("thread/settings/updated") => self.service.model_fact(&session,&runtime.instance_id,&event["params"]["threadSettings"]).await?,
                        Some("turn/started") => {
                            if let Ok(turns) = self.service.turns(&connection,thread).await {
                                self.service.reconcile_turns(&session,&runtime,&turns).await?;
                            }
                        }
                        Some("turn/completed") => self.service.turn_fact(&session,&runtime.instance_id,&event["params"]["turn"],"notification").await?,
                        Some("thread/archived") => { self.service.archived(&session,&runtime).await?; subscribed.remove(thread); }
                        Some("thread/unarchived") => {
                            self.service.connection_state(&session,&runtime.instance_id,"reconciling").await?;
                            subscribed.remove(thread);
                        }
                        Some("thread/status/changed") if event.pointer("/params/status/type").and_then(Value::as_str) == Some("notLoaded") => {
                            self.service.connection_state(&session,&runtime.instance_id,"unavailable").await?;
                            subscribed.remove(thread);
                        }
                        _ => {}
                    }
                }
                target = targets.recv() => {
                    if let Ok(target) = target { self.target(&runtime,target).await?; }
                }
            }
        }
    }

    async fn target(&self, runtime: &CodexRuntime, target: TuiTarget) -> Result<()> {
        let Some(thread_id) = target.thread["id"].as_str() else {
            return Ok(());
        };
        if !target.connected {
            sqlx::query("UPDATE codex_tui_bindings SET connected=FALSE WHERE owner_session_id=? AND runtime_instance_id=? AND connection_id=?").bind(&target.owner_session_id).bind(&runtime.instance_id).bind(&target.connection_id).execute(&self.service.pool).await?;
            return Ok(());
        }
        let observation = self
            .service
            .observed_session(&self.root, runtime, &target.thread);
        let session = pontia_application::native_sessions::NativeSessionService::new(
            self.service.event_ingest.clone(),
        )
        .resolve_observed_session("codex", thread_id, observation)
        .await?;
        sqlx::query("UPDATE codex_tui_bindings SET target_session_id=?,connected=TRUE,runtime_instance_id=?,connection_id=? WHERE owner_session_id=?")
            .bind(&session).bind(&runtime.instance_id).bind(&target.connection_id).bind(&target.owner_session_id).execute(&self.service.pool).await?;
        Ok(())
    }
}
