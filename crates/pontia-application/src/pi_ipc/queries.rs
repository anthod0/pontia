use crate::{AgentProfileService, AppState, ExternalQueryService};
use pontia_core::{Error, Result};
use pontia_runtime::pi_control::RpcRequest;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionQuery {
    session_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileQuery {
    profile_id: String,
    version: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceQuery {}

pub(super) async fn dispatch(state: &AppState, request: &RpcRequest) -> Result<Value> {
    match request.method.as_str() {
        "session.get" => {
            let query: SessionQuery = serde_json::from_value(request.params.clone())?;
            let session = ExternalQueryService::new(state.db())
                .get_session(&query.session_id)
                .await?
                .ok_or_else(|| {
                    Error::NotFound(format!("session {} not found", query.session_id))
                })?;
            Ok(json!({"session": session}))
        }
        "profile.get" => {
            let query: ProfileQuery = serde_json::from_value(request.params.clone())?;
            let service = AgentProfileService::new(state.db());
            let profile = match query.version {
                Some(version) => service.get_version(&query.profile_id, &version).await?,
                None => service.get_latest(&query.profile_id).await?,
            }
            .ok_or_else(|| {
                Error::NotFound(format!("agent profile {} not found", query.profile_id))
            })?;
            Ok(json!({"agent_profile": profile}))
        }
        "workspaces.list" => {
            let _: WorkspaceQuery = serde_json::from_value(request.params.clone())?;
            let workspaces = ExternalQueryService::new(state.db())
                .list_workspaces()
                .await?;
            Ok(json!({"workspaces": workspaces}))
        }
        _ => Err(Error::Domain("Unknown Pi query method".into())),
    }
}
