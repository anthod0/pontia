pub use pontia_config::FilePickerConfig;

mod agent_bindings;
mod agent_events;
mod agent_profiles;
pub mod app;
mod branch_replay;
mod git_status;
mod idempotency;
mod inbox;
pub mod ingestion;
mod pi_control;
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
    AgentBinding, AgentBindingCurrentTurn, AgentBindingService, AgentBindingSessionContext,
    UpsertAgentBindingRequest,
};
pub use agent_events::AgentEventBroker;
pub use agent_profiles::{
    AgentProfileCommandOutcome, AgentProfileService, ExecutionProfileView,
    UpsertExecutionProfileRequest,
};
pub use app::{AppState, initialize};
pub use branch_replay::{BranchReplayService, ResolveBranchReplayRequest, ResolvedBranchReplay};
pub use git_status::{GitRefreshCoordinator, WorkspaceGitStatusService};
pub use idempotency::{IdempotencyCoordinator, IdempotencyOutcome};
pub use inbox::{InboxCommandOutcome, InboxCommandService, SubmitInboxMessageRequest};
pub use ingestion::{
    EventIngestResult, EventIngestService, EventReportNormalizer, InternalEventValidationService,
    PontiaEvent, PontiaEventSource, PontiaEventType, ReportedFact,
};
pub use pi_control::PiGracefulExitService;
pub use queries::ExternalQueryService;
pub use raw_transcripts::{
    TurnTimelineDirection, TurnTimelineGroup, TurnTimelineItem, TurnTimelinePage,
    TurnTimelineService, TurnTimelineServiceError, TurnTreeHistoryPage, TurnTreeUpdatesPage,
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
pub use workspaces::{WorkspaceRecord, get_workspace_record, upsert_workspace};
