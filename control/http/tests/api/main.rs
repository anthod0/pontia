#[path = "../support/generic_client.rs"]
mod generic_client;
#[allow(dead_code)]
#[path = "../support/http.rs"]
mod http;
#[path = "../support/task_state.rs"]
mod task_state;
#[path = "../support/test_app.rs"]
mod test_app;

mod agent_profile;
mod approval;
mod external_event_stream;
mod external_queries;
mod health;
mod internal_event;
mod raw_transcript;
mod runtime_binding;
mod runtime_lifecycle;
mod session_create;
mod session_inbox;
mod session_workspace_linking;
mod task_creation;
mod task_lifecycle;
mod test_app_support;
mod turn_submit;
mod workflow_run;
mod workflow_submission;
mod workspace;
