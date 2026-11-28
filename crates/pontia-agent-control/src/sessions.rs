mod agent_bindings;
mod observations;
pub use agent_bindings::{
    AgentBinding, AgentBindingService, AgentBindingSessionContext, UpsertAgentBindingRequest,
};
pub(crate) use agent_bindings::{
    register_agent_binding_for_ready_event_in_tx, upsert_agent_binding_in_tx,
};
pub use observations::{
    NativeSessionIdentity, NativeSessionObservation, NativeSessionService, NativeSubscription,
};
use std::{collections::BTreeMap, path::PathBuf};

use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;

mod commands;
mod compaction;
mod fork;
pub mod instructions;
mod reasoning_effort;
pub use fork::{ForkSessionOutcome, ForkSessionRequest};
mod models;
pub use compaction::CompactionAccepted;
pub use models::{ReasoningEffortOption, SessionModel};
pub use reasoning_effort::SetSessionReasoningEffortRequest;
mod lifecycle;
mod runtime_binding;

mod persistence;
mod validation;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateSessionRequest {
    #[serde(skip)]
    pub launch_mode: pontia_core::LaunchMode,
    #[serde(default)]
    pub mode: pontia_core::SessionMode,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    #[serde(default, deserialize_with = "deserialize_client_type")]
    pub client_type: String,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub workspace_id: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub system_instructions: Option<String>,
    #[serde(default)]
    pub include_standalone_session_instructions: bool,
    #[serde(default)]
    pub metadata: Value,
    pub initial_task: Option<InitialTaskRequest>,
    #[serde(skip)]
    pub runtime_environment: BTreeMap<String, String>,
    #[serde(skip)]
    pub credential_env: Option<String>,
    #[serde(skip)]
    pub ownership: Option<crate::SessionOwnership>,
}

fn deserialize_client_type<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    let client = String::deserialize(deserializer)?;
    if client.is_empty() {
        return Err(serde::de::Error::custom("client_type cannot be empty"));
    }
    Ok(client)
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UpdateSessionRequest {
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct InitialTaskRequest {
    pub input: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateSessionOutcome {
    pub data: Value,
    pub duplicate: bool,
}

impl CreateSessionOutcome {
    pub fn session_id(&self) -> Option<&str> {
        self.data.get("session")?.get("session_id")?.as_str()
    }
}

#[derive(Clone)]
pub struct SessionCommandService {
    pool: SqlitePool,
    event_ingest: crate::EventIngestService,
    queries: crate::ControlQueryService,
    clients: crate::clients::ClientExecutionService,
    turns: crate::TurnCommandService,
    inbox: std::sync::Arc<crate::InboxCommandService>,
    pontia_home: PathBuf,
    creation_gate: std::sync::Arc<tokio::sync::Mutex<()>>,
    fork_gate: std::sync::Arc<tokio::sync::Mutex<()>>,
}

impl SessionCommandService {
    pub fn queries(&self) -> crate::ControlQueryService {
        self.queries.clone()
    }

    pub(crate) fn new(
        pool: SqlitePool,
        event_ingest: crate::EventIngestService,
        queries: crate::ControlQueryService,
        clients: crate::clients::ClientExecutionService,
        turns: crate::TurnCommandService,
        inbox: std::sync::Arc<crate::InboxCommandService>,
        pontia_home: PathBuf,
    ) -> Self {
        Self {
            creation_gate: std::sync::Arc::default(),
            fork_gate: std::sync::Arc::default(),
            pool,
            event_ingest,
            queries,
            clients,
            turns,
            inbox,
            pontia_home,
        }
    }
}

#[cfg(test)]
mod auto_exit_tests;
