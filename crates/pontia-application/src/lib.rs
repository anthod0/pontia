//! Application services for Session, Turn, Inbox, and runtime control.
//!
//! `app::AppState` assembles shared service lifetimes. Business services use the
//! client contracts for execution and `ingestion` for durable facts; committed
//! facts trigger ordered notifications and Inbox association before scheduling.

pub use pontia_config::FilePickerConfig;

mod agent_events;
mod agent_profiles;
pub mod app;
mod branch_replay;
pub mod clients;
pub mod control;
mod git_status;
mod idempotency;
mod inbox;
pub mod ingestion;
pub mod live_output;
pub mod queries;
mod raw_transcripts;
pub mod runtime;
pub mod sessions;
pub mod tasks;
pub mod turns;
pub mod views;
pub mod workspaces;

pub use agent_events::AgentEventBroker;
pub use agent_profiles::{
    AgentProfileCommandOutcome, AgentProfileService, CodexProfileBinding, ExecutionProfileView,
    UpsertExecutionProfileRequest,
};
pub use app::AppState;
pub use branch_replay::{BranchReplayService, ResolveBranchReplayRequest, ResolvedBranchReplay};
pub use client_contract::{ClientControlChannel, ClientControlOperation};
pub use clients::ClientControlService;
pub use control::ControlCommandOutcome;
pub use git_status::{GitRefreshCoordinator, WorkspaceGitStatusService};
pub use idempotency::{IdempotencyCoordinator, IdempotencyOutcome};
pub use inbox::{
    InboxCommandOutcome, InboxCommandService, RetryInboxMessageRequest, SubmitInboxMessageRequest,
};
pub use ingestion::{
    EventIngestResult, EventIngestService, EventReportError, EventReportNormalizer,
    InternalEventValidationService, PontiaEvent, PontiaEventSource, PontiaEventType, ReportedFact,
};
pub use live_output::{
    LiveOutputBatch, LiveOutputClose, LiveOutputCloseReason, LiveOutputIdentity, LiveOutputItem,
    LiveOutputProducer, LiveOutputPublishOutcome, LiveOutputService, LiveOutputSnapshot,
    LiveOutputSnapshotReplacement, LiveOutputStreamEvent, LiveOutputSubscription, LiveOutputUpdate,
};
pub use queries::ExternalQueryService;
pub use raw_transcripts::{
    TurnTimelineDirection, TurnTimelineGroup, TurnTimelineItem, TurnTimelinePage,
    TurnTimelineService, TurnTimelineServiceError, TurnTreeHistoryPage, TurnTreeUpdatesPage,
};
pub use runtime::{RuntimeBindingUpsertRequest, RuntimeBindingUpsertService};
pub use runtime::{RuntimeObservationService, RuntimeReadinessService};
pub use sessions::{
    AgentBinding, AgentBindingService, AgentBindingSessionContext, UpsertAgentBindingRequest,
};
pub use sessions::{
    CreateSessionOutcome, CreateSessionRequest, InitialTaskRequest, SessionCommandService,
    UpdateSessionRequest,
};
pub use tasks::{CreateTaskOutcome, TaskCommandService};
pub use turns::TurnCommandService;
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

pub(crate) use app::default_client_type;
pub use workspaces::{WorkspaceRecord, get_workspace_record, upsert_workspace};

pub mod client_contract;
