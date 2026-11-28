mod bot_context;
pub use bot_context::{
    dreaming_command, helper_command, profile_command, session_messages, update_user,
    workspace_command,
};
mod bot_memory;
pub use bot_memory::{get_user_file, list_memories, memory_command, replace_user_file};
mod agent_clients;
pub mod bots;
pub use agent_clients::{
    agent_client_setup_status, cancel_agent_client_operation, delete_agent_client,
    get_agent_client, get_agent_client_operation, install_agent_client,
    latest_agent_client_operation, list_agent_client_models, list_agent_clients,
    patch_agent_client, setup_agent_client, stream_agent_client_operation,
};
mod agent_profiles;
mod client_auth;
pub(crate) use client_auth::no_store;
pub use client_auth::{
    cancel_client_auth_session, get_client_auth, get_client_auth_session, logout_client_auth,
    probe_client_auth, start_client_auth_session, stream_client_auth_session,
    submit_client_auth_input,
};
mod auth;
mod authentication;
mod branch_replay;
mod dashboard_events;
mod events;
mod git;
mod git_status;
pub(crate) mod git_write;
mod idempotency;
mod interactions;
mod messages;
pub use interactions::{get_interaction, list_interactions, respond_interaction};
mod live_output;
mod response;
mod session_guard;
mod sessions;
mod shutdown;
pub(crate) use shutdown::shutdown_agents;
mod timeline;
mod turns;
mod workflows;
mod workspaces;

pub use agent_profiles::{
    create_agent_profile, delete_agent_profile, get_agent_profile, list_agent_profiles,
    update_agent_profile,
};
pub use auth::validate_auth;
pub(crate) use authentication::authenticate;
pub use branch_replay::submit_branch_replay;
pub use dashboard_events::stream_dashboard_events;
pub use events::{stream_session_events, stream_turn_events};
pub use git::{
    get_workspace_git_branches, get_workspace_git_changes, get_workspace_git_commit,
    get_workspace_git_commit_diff, get_workspace_git_commits, get_workspace_git_diff,
};
pub use git_status::{get_workspace_git_status, refresh_workspace_git_status};
pub use live_output::stream_live_output;
pub use messages::{cancel_queued_message, list_queue, send_message};
pub use response::{ApiError, ApiResponse};
pub use sessions::{
    archive_session, compact_session, create_session, exit_session, fork_session, get_session,
    interrupt_session, pin_session, restart_session, resume_session, session_creation_options,
    session_overview, set_session_reasoning_effort, unarchive_session, unpin_session,
    update_session,
};
pub use timeline::{
    get_turn_timeline, get_turn_tree_history, get_turn_tree_updates, read_session_messages,
};
pub use turns::{get_turn, interrupt_turn, list_session_events, list_turn_events, list_turns};
pub use workflows::{
    WorkflowRunRequest, WorkflowSubmissionRequest, WorkflowTerminationRequest, get_workflow,
    get_workflow_context, get_workflow_timeline, list_workflows, pause_workflow, resume_workflow,
    retry_workflow, run_workflow, submit_workflow_output, terminate_workflow_node,
};
pub use workspaces::{
    delete_workspace, get_workspace, list_workspace_root_entries, list_workspace_roots,
    list_workspaces, pick_workspace_files, register_workspace, rename_workspace,
};

mod uploads;
pub use uploads::{append_upload, complete_upload, create_upload, get_upload};

mod session_files;
pub use session_files::path_file_content;
