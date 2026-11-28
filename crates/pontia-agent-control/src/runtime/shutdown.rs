mod command;
pub use command::{Operation, ShutdownRequest, ShutdownUpdate};

use crate::{client_contract::PreparedServiceShutdown, clients::ClientRegistry};
use std::{path::Path, time::Duration};

use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, ProcessFingerprint, ProcessObservation};
use serde_json::json;
use sqlx::{FromRow, SqlitePool};

use crate::{EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType};

pub struct Shutdown {
    lock: pontia_runtime::launch_lock::LaunchLock,
    events: Option<EventIngestService>,
    runtimes: Vec<ShutdownRuntime>,
    services: Vec<PreparedServiceShutdown>,
}

impl Shutdown {
    pub async fn inspect(
        root: &Path,
        clients: &ClientRegistry,
        database: impl Future<Output = Result<Option<SqlitePool>>>,
    ) -> Result<Self> {
        let lock = pontia_runtime::launch_lock::shutdown(root)?;
        let events = database.await?.map(|pool| {
            crate::AgentControl::builder(pool, root.to_owned())
                .clients(clients.clone())
                .build()
                .event_ingest_service()
        });
        let mut services = Vec::new();
        for service in clients.registrations().filter_map(|c| c.service.as_ref()) {
            if let Some(prepared) = service.inspect_shutdown(root, events.as_ref()).await? {
                services.push(prepared);
            }
        }
        let runtimes = match &events {
            Some(events) => ShutdownRuntime::list(&events.db(), root, clients).await?,
            None => Vec::new(),
        };
        Ok(Self {
            lock,
            events,
            runtimes,
            services,
        })
    }

    fn listing(&self) -> Vec<String> {
        self.runtimes
            .iter()
            .map(|r| r.session().label())
            .chain(self.services.iter().flat_map(|s| s.listing.iter().cloned()))
            .collect()
    }

    async fn terminate(&mut self) -> Result<Vec<String>> {
        if let Some(events) = &self.events {
            for runtime in self.runtimes.drain(..) {
                runtime.terminate(events).await?;
            }
        }
        let mut warnings = Vec::new();
        for service in self.services.drain(..) {
            if let Some(warning) = service.terminate.await? {
                warnings.push(warning);
            }
        }
        Ok(warnings)
    }
}

#[derive(FromRow, Debug)]
pub struct ShutdownSession {
    pub session_id: String,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub client_type: String,
}

impl ShutdownSession {
    pub fn label(&self) -> String {
        let title = self
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("Untitled Session");
        format!(
            "{title:?} ({}, {})",
            self.client_type,
            self.workspace.as_deref().unwrap_or("No Workspace")
        )
    }

    pub async fn exited(
        &self,
        events: &EventIngestService,
        runtime: serde_json::Value,
    ) -> Result<()> {
        events
            .ingest_pontia_event(PontiaEvent::new(
                &self.session_id,
                None,
                PontiaEventSource::RuntimeManager,
                &self.client_type,
                PontiaEventType::SessionExited,
                runtime,
            ))
            .await?;
        Ok(())
    }
}

#[derive(FromRow)]
struct Record {
    #[sqlx(flatten)]
    session: ShutdownSession,
    runtime_id: String,
    role: String,
    process_fingerprint: Option<String>,
    tmux_socket_path: Option<String>,
    tmux_pane_id: Option<String>,
}

struct ShutdownRuntime {
    record: Record,
    process: Option<ProcessFingerprint>,
    pane: Option<ProcessFingerprint>,
    coupled_runtime: bool,
}

impl ShutdownRuntime {
    fn session(&self) -> &ShutdownSession {
        &self.record.session
    }

    async fn list(
        pool: &SqlitePool,
        root: &std::path::Path,
        clients: &ClientRegistry,
    ) -> Result<Vec<Self>> {
        let mut query = sqlx::QueryBuilder::new(
            "SELECT s.session_id,s.title,s.client_type,COALESCE(w.display_path,s.workspace_ref) AS workspace,
                    r.runtime_id,r.role,r.process_fingerprint,r.tmux_socket_path,r.tmux_pane_id
             FROM session_runtimes r JOIN sessions s USING(session_id)
             LEFT JOIN workspaces w USING(workspace_id)
             WHERE FALSE",
        );
        for client in clients.registrations() {
            let adapter = &client.spec.adapter;
            let Some(role) = adapter.control_runtime_role else {
                continue;
            };
            query
                .push(" OR (s.client_type=")
                .push_bind(client.spec.client_type)
                .push(" AND r.role=")
                .push_bind(role);
            if adapter.lifecycle.shutdown_requires_managed_launch {
                query.push(" AND (SELECT e.source FROM events e WHERE e.session_id=s.session_id AND
                    e.event_type IN ('session.starting','session.resuming') ORDER BY e.rowid DESC LIMIT 1)='external_api'");
            }
            query.push(")");
        }
        query.push(" ORDER BY s.session_id,r.runtime_id");
        let records: Vec<Record> = query.build_query_as().fetch_all(pool).await?;
        let snapshot = pontia_runtime::process::ProcessSnapshot::capture();
        let mut sockets = std::collections::HashMap::new();
        let log = pontia_runtime::pontia_log_paths(root).runtime_log;
        let mut live = Vec::new();
        for record in records {
            let process: Option<ProcessFingerprint> = record
                .process_fingerprint
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?;
            let process = match process.as_ref().map(|p| snapshot.observe(p)) {
                Some(ProcessObservation::Alive) => process,
                Some(ProcessObservation::Unknown) => {
                    return Err(Error::ControlUnknown(format!(
                        "cannot verify runtime {}",
                        record.runtime_id
                    )));
                }
                _ => None,
            };
            let pane = if let Some((socket, pane)) = record
                .tmux_socket_path
                .as_deref()
                .zip(record.tmux_pane_id.as_deref())
            {
                let panes = match sockets.entry(socket.to_string()) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(pontia_runtime::pane_processes(socket)?)
                    }
                };
                match panes.get(pane) {
                    Some(pane) => pane.owner(
                        &record.runtime_id,
                        process.as_ref(),
                        &log,
                        &record.session.session_id,
                    )?,
                    None => None,
                }
            } else {
                None
            };
            if pane.is_some() || process.is_some() {
                let spec = clients
                    .spec(&record.session.client_type)
                    .expect("selected a registered control runtime");
                live.push(Self {
                    coupled_runtime: spec.adapter.lifecycle.coupled_runtime,
                    record,
                    process,
                    pane,
                });
            }
        }
        Ok(live)
    }

    async fn terminate(self, events: &EventIngestService) -> Result<()> {
        let r = self.record;
        if let Some(pane) = &self.pane {
            terminate_with_tree(pane, || {
                pontia_runtime::kill_owned_pane(
                    r.tmux_socket_path.as_deref().unwrap(),
                    r.tmux_pane_id.as_deref().unwrap(),
                    pane,
                )
            })
            .await?;
        }
        if let Some(process) = self.process {
            terminate_tree(&process).await?;
        }
        let fact = json!({"runtime_id":r.runtime_id,"role":r.role,"process_fingerprint":r.process_fingerprint,"reason":"management_exit"});
        if !self.coupled_runtime {
            events
                .ingest_pontia_event(PontiaEvent::new(
                    &r.session.session_id,
                    None,
                    PontiaEventSource::RuntimeManager,
                    &r.session.client_type,
                    PontiaEventType::RuntimeExited,
                    fact.clone(),
                ))
                .await?;
        }
        r.session.exited(events, fact).await
    }
}

pub async fn terminate_tree(process: &ProcessFingerprint) -> Result<()> {
    terminate_with_tree(process, || GenericRuntimeManager.terminate_process(process)).await
}

async fn terminate_with_tree(
    process: &ProcessFingerprint,
    terminate: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let mut tree = process.descendants();
    tree.push(process.clone());
    terminate()?;
    if wait_exited(&tree, Duration::from_secs(6)).await {
        return Ok(());
    }
    // The relay and Claude host normally reap their own child on SIGTERM.
    for child in &tree {
        GenericRuntimeManager.kill_process(child)?;
    }
    if !wait_exited(&tree, Duration::from_secs(5)).await {
        return Err(Error::ControlUnknown(format!(
            "could not confirm exit of process tree {}",
            process.pid
        )));
    }
    Ok(())
}

async fn wait_exited(tree: &[ProcessFingerprint], timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let snapshot = pontia_runtime::process::ProcessSnapshot::capture();
        if tree
            .iter()
            .all(|p| snapshot.observe(p) == ProcessObservation::Exited)
        {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
