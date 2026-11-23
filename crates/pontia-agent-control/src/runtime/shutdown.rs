use std::time::Duration;

use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, ProcessFingerprint, ProcessObservation};
use serde_json::json;
use sqlx::{FromRow, SqlitePool};

use crate::{EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType};

#[derive(FromRow, Debug)]
pub struct ShutdownSession {
    pub session_id: String,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub client_type: String,
}

impl ShutdownSession {
    pub async fn background_codex(pool: &SqlitePool) -> Result<Vec<Self>> {
        let sessions: Vec<Self> = sqlx::query_as(
            "SELECT s.session_id, s.title, s.client_type, COALESCE(w.display_path, s.workspace_ref) AS workspace
             FROM sessions s LEFT JOIN workspaces w USING(workspace_id)
             WHERE s.client_type='codex' AND s.state NOT IN ('exited','error')
             ORDER BY s.session_id",
        ).fetch_all(pool).await?;
        let queries = crate::ControlQueryService::new(pool.clone());
        let mut background = Vec::new();
        for session in sessions {
            if queries.session_launch_mode(&session.session_id).await?
                == pontia_core::LaunchMode::Background
            {
                background.push(session);
            }
        }
        Ok(background)
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

pub struct ShutdownRuntime {
    record: Record,
    process: Option<ProcessFingerprint>,
    pane: Option<ProcessFingerprint>,
}

impl ShutdownRuntime {
    pub fn session(&self) -> &ShutdownSession {
        &self.record.session
    }

    pub async fn list(pool: &SqlitePool, root: &std::path::Path) -> Result<Vec<Self>> {
        // Native Pi registrations also have a fingerprint. Only its latest Pontia
        // launch (as opposed to a user reopening that conversation) confers ownership.
        let records: Vec<Record> = sqlx::query_as(r#"SELECT s.session_id,s.title,s.client_type,COALESCE(w.display_path,s.workspace_ref) AS workspace,
            r.runtime_id,r.role,r.process_fingerprint,r.tmux_socket_path,r.tmux_pane_id
            FROM session_runtimes r JOIN sessions s USING(session_id)
            LEFT JOIN workspaces w USING(workspace_id)
            WHERE (s.client_type='acp' AND r.role='acp') OR
                  (s.client_type='claude-agent' AND r.role='claude_sdk_host') OR
                  (s.client_type='pi' AND r.role='tui' AND
                   (SELECT e.source FROM events e WHERE e.session_id=s.session_id AND
                    e.event_type IN ('session.starting','session.resuming') ORDER BY e.rowid DESC LIMIT 1)='external_api')
            ORDER BY s.session_id,r.runtime_id"#).fetch_all(pool).await?;
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
                live.push(Self {
                    record,
                    process,
                    pane,
                });
            }
        }
        Ok(live)
    }

    pub async fn terminate(self, events: &EventIngestService) -> Result<()> {
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
        if r.session.client_type != "pi" {
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

/// Open only an existing control-plane database. A first-run home has no records.
pub async fn existing_database(url: &str) -> Result<Option<SqlitePool>> {
    let options: sqlx::sqlite::SqliteConnectOptions = url.parse()?;
    if !options.get_filename().try_exists()? {
        return Ok(None);
    }
    Ok(Some(SqlitePool::connect_with(options).await?))
}
