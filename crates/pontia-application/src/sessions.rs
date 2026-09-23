mod agent_bindings;
mod observations;
pub use agent_bindings::{
    AgentBinding, AgentBindingService, AgentBindingSessionContext, UpsertAgentBindingRequest,
};
pub(crate) use agent_bindings::{
    register_agent_binding_for_ready_event_in_tx, upsert_agent_binding_in_tx,
};
pub use observations::{NativeSessionIdentity, NativeSessionObservation, NativeSessionService};
use std::{collections::BTreeMap, path::PathBuf};

use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::default_client_type;

mod commands;
mod models;
pub use models::{SessionModel, SessionModels, SetSessionModelRequest};
mod lifecycle;
mod runtime_binding;

mod persistence;
mod validation;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateSessionRequest {
    #[serde(default = "default_client_type")]
    pub client_type: String,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub workspace_id: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub initial_task: Option<InitialTaskRequest>,
    #[serde(skip)]
    pub runtime_environment: BTreeMap<String, String>,
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
    queries: crate::ExternalQueryService,
    clients: crate::clients::ClientExecutionService,
    turns: crate::TurnCommandService,
    inbox: std::sync::Arc<crate::InboxCommandService>,
    pontia_home: PathBuf,
}

impl SessionCommandService {
    pub(crate) fn new(
        pool: SqlitePool,
        event_ingest: crate::EventIngestService,
        queries: crate::ExternalQueryService,
        clients: crate::clients::ClientExecutionService,
        turns: crate::TurnCommandService,
        inbox: std::sync::Arc<crate::InboxCommandService>,
        pontia_home: PathBuf,
    ) -> Self {
        Self {
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
