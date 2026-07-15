use std::str::FromStr;

use pontia_agent_clients as agent_clients;
pub use pontia_config::FilePickerConfig;
use pontia_core::{
    domain::{
        DomainEvent, EventSource, EventType, ReportedEvent, SessionProjection, SessionState,
        TurnProjection, TurnState,
    },
    error::{Error, Result},
    ids::{new_event_id, new_message_id, new_session_id, new_turn_id},
};
use pontia_runtime::{AgentInput, GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;

mod agent_bindings;
pub mod app;
mod events;
mod git_status;
mod inbox;
pub mod ingestion;
mod mapping;
pub mod queries;
mod raw_transcripts;
pub mod runtime;
pub mod runtime_control;
pub mod sessions;
pub mod tasks;
pub mod turns;
pub mod views;
pub mod workspaces;

pub use agent_bindings::{
    AgentBinding, AgentBindingCurrentTurn, AgentBindingService, UpsertAgentBindingRequest,
};
pub use app::{AppState, initialize};
pub(crate) use events::nested_string;
pub use events::{EventIngestResult, EventIngestService, InternalEventValidationService};
pub use git_status::{GitRefreshCoordinator, WorkspaceGitStatusService};
pub use inbox::{InboxCommandOutcome, InboxCommandService, SubmitInboxMessageRequest};
pub use queries::ExternalQueryService;
pub use raw_transcripts::{
    AgentBindingResolveRequest, AgentBindingResolver, RawTranscriptParser, RawTranscriptService,
    RawTranscriptServiceError, RawTranscriptTimelineErrorCode, ResolvedAgentBinding, TimelineItem,
    TimelineItemDetailPage, TimelineItemDetailRequest, TimelinePage, TimelinePageRequest,
    resolve_and_parse_timeline_page,
};
pub use runtime::{RuntimeBindingUpsertRequest, RuntimeBindingUpsertService};
pub use runtime::{RuntimeObservationService, RuntimeReadinessService};
pub use runtime_control::{ControlCommandOutcome, RuntimeControlService};
pub use sessions::{
    CreateSessionOutcome, CreateSessionRequest, InitialTaskRequest, SessionCommandService,
    UpdateSessionRequest,
};
pub use tasks::{CreateTaskOutcome, TaskCommandService};
pub use turns::{CurrentTurnClaimRequest, CurrentTurnClaimService, TurnCommandService};
pub use views::{
    ContextUsageCapability, ContextUsageView, EventStreamItem, EventStreamScope, EventView,
    InboxInputView, InboxMessageView, SessionCapabilities, SessionLineageView, SessionView,
    TaskEventStreamItem, TaskEventView, TaskView, TurnInputView, TurnOutputView, TurnView,
    WorkspaceGitStatusView, WorkspaceView,
};
pub use workspaces::{
    FilePickerFileView, FilePickerResultView, RegisterWorkspaceRequest, RenameWorkspaceRequest,
    WorkspaceBrowserConfig, WorkspaceBrowserService, WorkspaceDirectoryEntryView,
    WorkspaceDirectoryListingView, WorkspaceRootConfig, WorkspaceRootView,
};

pub(crate) use app::{default_client_type, is_supported_client_type};
pub(crate) use mapping::*;
pub use workspaces::{WorkspaceRecord, get_workspace_record, upsert_workspace};
