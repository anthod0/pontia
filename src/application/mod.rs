use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
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

mod agent_bindings;
mod agent_profiles;
mod agent_tools;
mod artifacts;
mod dag;
mod dag_models;
mod dag_planning;
mod dag_run_result;
mod dag_scheduler;
mod dag_validator;
mod events;
mod git_status;
mod graph;
mod inbox;
mod mapping;
mod prompt_rendering;
mod queries;
mod raw_transcripts;
mod runtime_bindings;
mod runtime_control;
mod runtime_observation;
mod runtime_readiness;
mod sessions;
mod state;
mod tasks;
mod turns;
mod views;
mod workspaces;

pub use agent_bindings::{AgentBinding, AgentBindingService, UpsertAgentBindingRequest};
pub use agent_profiles::{
    AgentProfileCommandOutcome, AgentProfileService, ExecutionProfileView,
    UpsertExecutionProfileRequest,
};
pub use agent_tools::{
    AgentPlanningRole, AgentToolContext, AgentToolContextResolver, AgentToolMode, AgentToolRequest,
    AgentToolResponse, AgentToolService,
};
pub use artifacts::{
    ArtifactContentService, ArtifactDiscoveryService, ArtifactRegistration,
    ArtifactRegistrationService,
};
pub use dag::DagService;
pub use dag_models::{
    DagPatch, DagPatchApplySummary, DagProposal, DagSignalRecord, PatchOperation,
    RaiseSignalPayload, SubmitPlanPayload, SubmitResultPayload, WorkItemDraft, WorkItemEdgeDraft,
    WorkItemRecord, WorkItemRunRecord,
};
pub use dag_planning::{DagPlanningOutcome, DagPlanningService, DagPlanningTurn};
pub use dag_run_result::DagRunResultService;
pub use dag_scheduler::{DagSchedulerDispatch, DagSchedulerOutcome, DagSchedulerService};
pub use events::{EventIngestResult, EventIngestService};
pub(crate) use events::{nested_array_strings, nested_string, remove_internal_metadata_fields};
pub use git_status::{GitRefreshCoordinator, WorkspaceGitStatusService};
#[cfg(feature = "lbug")]
pub use graph::LbugDagGraphStore;
pub use graph::{
    AddWorkItemEdgeRequest, GraphEdgeKind, GraphProjectionService, GraphRuntimeConfig, SignalNode,
    TaskGraphSnapshot, TaskNode, TaskProvenance, UpsertSignalRequest, UpsertTaskRequest,
    UpsertWorkItemRequest, WorkItemEdgeRecord, WorkItemNode,
};
pub use inbox::{InboxCommandOutcome, InboxCommandService, SubmitInboxMessageRequest};
pub use queries::ExternalQueryService;
pub use raw_transcripts::{
    AgentBindingResolveRequest, AgentBindingResolver, PiAgentBindingResolver, PiJsonlParser,
    RawTranscriptParser, ResolvedAgentBinding, TimelineItem, TimelineItemDetailPage,
    TimelineItemDetailRequest, TimelinePage, TimelinePageRequest, resolve_and_parse_timeline_page,
};
pub use runtime_bindings::{RuntimeBindingUpsertRequest, RuntimeBindingUpsertService};
pub use runtime_control::{ControlCommandOutcome, RuntimeControlService};
pub use runtime_observation::{AdapterEventOutboxService, RuntimeObservationService};
pub use runtime_readiness::RuntimeReadinessService;
pub use sessions::{
    CreateSessionOutcome, CreateSessionRequest, InitialTaskRequest, SessionCommandService,
    UpdateSessionRequest,
};
pub use state::{AppState, initialize};
pub use tasks::{CreateDagTaskRequest, CreateTaskOutcome, HumanSignalRequest, TaskCommandService};
pub use turns::TurnCommandService;
pub use views::*;
pub use workspaces::{
    RegisterWorkspaceRequest, RenameWorkspaceRequest, WorkspaceBrowserConfig,
    WorkspaceBrowserService, WorkspaceDirectoryEntryView, WorkspaceDirectoryListingView,
    WorkspaceRootConfig, WorkspaceRootView,
};

pub(crate) use mapping::*;
pub(crate) use workspaces::{WorkspaceRecord, get_workspace_record, upsert_workspace};

use std::sync::{OnceLock, RwLock};

fn default_client_type_store() -> &'static RwLock<String> {
    static DEFAULT_CLIENT_TYPE: OnceLock<RwLock<String>> = OnceLock::new();
    DEFAULT_CLIENT_TYPE.get_or_init(|| RwLock::new("pi".to_string()))
}

pub(crate) fn set_default_client_type(client_type: String) {
    let mut guard = default_client_type_store()
        .write()
        .expect("default client type lock poisoned");
    *guard = client_type;
}

fn default_client_type() -> String {
    default_client_type_store()
        .read()
        .expect("default client type lock poisoned")
        .clone()
}

fn is_supported_client_type(client_type: &str) -> bool {
    agent_clients::is_supported_client_type(client_type)
}
