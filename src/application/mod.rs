use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use time::format_description::well_known::Rfc3339;

use crate::{
    adapters::ArtifactRegistration,
    agent_clients,
    config::AppConfig,
    domain::{
        DomainEvent, EventSource, EventType, ProjectionState, SessionProjection, SessionState,
        TurnProjection, TurnState,
    },
    error::{Error, Result},
    ids::{
        new_event_id, new_message_id, new_session_id, new_task_id, new_turn_id, new_workspace_id,
    },
    runtime::{AgentInput, GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult},
    storage::sqlite::{connect_sqlite, run_migrations},
};

mod agent_profiles;
mod artifacts;
mod events;
mod graph;
mod inbox;
mod mapping;
mod planner;
mod queries;
mod runtime_control;
mod runtime_observation;
mod runtime_readiness;
mod sessions;
mod state;
mod tasks;
mod turns;
mod views;
mod workspaces;

pub use agent_profiles::{
    AgentProfileCommandOutcome, AgentProfileService, ExecutionProfileView,
    UpsertExecutionProfileRequest,
};
pub use artifacts::{
    ArtifactContentService, ArtifactDiscoveryService, ArtifactRegistrationService,
};
pub use events::{EventIngestResult, EventIngestService};
pub(crate) use events::{nested_array_strings, nested_string, remove_internal_metadata_fields};
pub use graph::{GraphProjectionService, GraphRuntimeConfig, TaskProvenance};
pub use inbox::{InboxCommandOutcome, InboxCommandService, SubmitInboxMessageRequest};
pub use planner::{
    FakeTaskPlanner, PiTaskPlanner, PlannerDecision, PlannerDecisionStatus, PlannerInput,
    PlannerRuntimeConfig, SubmitPlannerInputRequest, TaskPlannerService,
};
pub use queries::ExternalQueryService;
pub use runtime_control::{ControlCommandOutcome, RuntimeControlService};
pub use runtime_observation::{PiAdapterEventOutboxService, RuntimeObservationService};
pub use runtime_readiness::RuntimeReadinessService;
pub use sessions::{
    CreateSessionOutcome, CreateSessionRequest, InitialTaskRequest, SessionCommandService,
};
pub use state::{AppState, initialize};
pub use tasks::{
    ConfirmTaskWorkspaceRequest, CreateTaskOutcome, CreateTaskRequest, TaskCommandService,
};
pub use turns::TurnCommandService;
pub use views::*;
pub use workspaces::{
    RegisterWorkspaceRequest, WorkspaceBrowserConfig, WorkspaceBrowserService,
    WorkspaceDirectoryEntryView, WorkspaceDirectoryListingView, WorkspaceRootConfig,
    WorkspaceRootView,
};

pub(crate) use mapping::*;
pub(crate) use workspaces::{WorkspaceRecord, get_workspace_record, upsert_workspace};

fn default_client_type() -> String {
    "generic".to_string()
}

fn is_supported_client_type(client_type: &str) -> bool {
    agent_clients::is_supported_client_type(client_type)
}
