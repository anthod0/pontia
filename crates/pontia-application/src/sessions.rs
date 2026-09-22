use std::{collections::BTreeMap, path::PathBuf};

use pontia_runtime::GenericRuntimeManager;
use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;

use crate::default_client_type;

mod commands;
mod dispatch;
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
    pi_control: Option<crate::PiControlService>,
    pontia_home: PathBuf,
    runtime: GenericRuntimeManager,
}

impl SessionCommandService {
    pub fn with_pi_control(mut self, pi_control: crate::PiControlService) -> Self {
        self.pi_control = Some(pi_control);
        self
    }

    pub fn new(event_ingest: crate::EventIngestService, pontia_home: PathBuf) -> Self {
        Self {
            pool: event_ingest.db(),
            event_ingest,
            pi_control: None,
            pontia_home,
            runtime: GenericRuntimeManager,
        }
    }
}
