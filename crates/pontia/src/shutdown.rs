use std::path::PathBuf;

use pontia_agent_control::{
    AgentControl, EventIngestService,
    runtime::shutdown::{ShutdownRuntime, ShutdownSession, existing_database, terminate_tree},
};
use pontia_client_codex::runtime::{BackgroundServer, background_server};
use pontia_config::AppConfig;
use serde_json::json;

pub struct Shutdown {
    _lock: pontia_runtime::launch_lock::LaunchLock,
    runtimes: Vec<ShutdownRuntime>,
    codex: BackgroundServer,
    codex_sessions: Vec<ShutdownSession>,
    events: Option<EventIngestService>,
}

impl Shutdown {
    pub async fn inspect(config: &AppConfig) -> Result<Self, String> {
        let root = &config.pontia_home;
        let lock = pontia_runtime::launch_lock::shutdown(root).map_err(|e| e.to_string())?;
        let codex = background_server(root).await.map_err(|e| e.to_string())?;
        let pool = existing_database(&config.database_url)
            .await
            .map_err(|e| e.to_string())?;
        let mut runtimes = vec![];
        let mut codex_sessions = vec![];
        let mut events = None;
        if let Some(pool) = pool {
            runtimes = ShutdownRuntime::list(&pool, root)
                .await
                .map_err(|e| e.to_string())?;
            if !codex.owned.is_empty() {
                codex_sessions = ShutdownSession::background_codex(&pool)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            events = Some(
                AgentControl::builder(pool, root.clone())
                    .clients(pontia_clients::registration(config))
                    .build()
                    .event_ingest_service(),
            );
        }
        Ok(Self {
            _lock: lock,
            runtimes,
            codex,
            codex_sessions,
            events,
        })
    }

    pub fn listing(&self) -> Vec<String> {
        let mut lines = self
            .runtimes
            .iter()
            .map(ShutdownRuntime::session)
            .chain(&self.codex_sessions)
            .map(|s| {
                let title = s
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .unwrap_or("Untitled Session");
                format!(
                    "{title:?} ({}, {})",
                    s.client_type,
                    s.workspace.as_deref().unwrap_or("No Workspace")
                )
            })
            .collect::<Vec<_>>();
        for process in &self.codex.owned {
            lines.push(format!("Codex background app-server (PID {})", process.pid));
        }
        if let Some(listener) = &self.codex.unverified_listener {
            lines.push(format!(
                "{} (ownership unverified)",
                listener_label(listener)
            ));
        }
        lines
    }

    pub fn require_force(&self, operation: &str, force: bool) -> Result<(), String> {
        let lines = self.listing();
        if force || lines.is_empty() {
            return Ok(());
        }
        Err(format!(
            "{operation} refused while agent processes are running:\n  - {}\nRun `pontia {operation} --force` to stop Pontia-started agents and continue.",
            lines.join("\n  - ")
        ))
    }

    pub async fn run<T>(
        self,
        operation: &str,
        force: bool,
        stop: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        self.require_force(operation, force)?;
        if let Some(events) = &self.events {
            for runtime in self.runtimes {
                runtime.terminate(events).await.map_err(|e| e.to_string())?;
            }
        }
        for process in &self.codex.owned {
            terminate_tree(process).await.map_err(|e| e.to_string())?;
        }
        if let Some(events) = &self.events {
            for session in &self.codex_sessions {
                session
                    .exited(events, json!({"reason":"management_exit"}))
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        if let Some(listener) = &self.codex.unverified_listener {
            eprintln!(
                "Warning: {} was left running because its ownership could not be verified. Inspect the listener with `lsof -U`, verify its PID and stop it manually with `kill -TERM <verified-PID>`.",
                listener_label(listener)
            );
        }
        stop()
    }
}

fn listener_label((path, pid): &(PathBuf, Option<u32>)) -> String {
    let pid = pid.map(|pid| format!(", PID {pid}")).unwrap_or_default();
    format!("Codex background listener at {}{pid}", path.display())
}
