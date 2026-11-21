use pontia_agent_control::ipc::rpc::RpcRequest;
use pontia_agent_control::{AgentControl, ControlQueryService, SessionInstructionsService};
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionQuery {
    session_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceQuery {}

pub(super) async fn dispatch(state: &AgentControl, request: &RpcRequest) -> Result<Value> {
    match request.method.as_str() {
        "session.get" => {
            let query: SessionQuery = serde_json::from_value(request.params.clone())?;
            let session = ControlQueryService::new(state.db())
                .with_clients(state.clients())
                .get_session(&query.session_id)
                .await?
                .ok_or_else(|| {
                    Error::NotFound(format!("session {} not found", query.session_id))
                })?;
            let binding = SessionInstructionsService::new(state.db())
                .session_instructions(&query.session_id)
                .await?;
            Ok(json!({"session": session, "session_instructions": binding}))
        }
        "workspaces.list" => {
            let _: WorkspaceQuery = serde_json::from_value(request.params.clone())?;
            let workspaces = ControlQueryService::new(state.db())
                .with_clients(state.clients())
                .list_workspaces()
                .await?;
            Ok(json!({"workspaces": workspaces}))
        }
        _ => Err(Error::Domain("Unknown Pi query method".into())),
    }
}
