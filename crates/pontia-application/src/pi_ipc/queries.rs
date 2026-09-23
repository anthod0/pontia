use super::Attach;
use crate::{
    AgentProfileService, AppState, CurrentTurnClaimRequest, CurrentTurnClaimService,
    ExternalQueryService,
};
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnClaim {
    session_id: String,
    runtime_instance_id: String,
    client_type: String,
}

pub(super) async fn dispatch(
    state: &AppState,
    request: &RpcRequest,
    identity: Option<&Attach>,
) -> Result<Value> {
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
        "turn.claim" => {
            let identity = identity.ok_or_else(|| {
                Error::StateConflict("Pi turn claim requires registration".into())
            })?;
            let query: TurnClaim = serde_json::from_value(request.params.clone())?;
            if query.session_id != identity.session_id
                || query.runtime_instance_id != identity.runtime_instance_id
                || query.client_type != "pi"
            {
                return Err(Error::StateConflict(
                    "Pi turn claim does not match its connection identity".into(),
                ));
            }
            let current_turn = CurrentTurnClaimService::new(state.db())
                .claim(
                    &query.session_id,
                    CurrentTurnClaimRequest {
                        runtime_instance_id: query.runtime_instance_id,
                        client_type: query.client_type,
                    },
                )
                .await?;
            Ok(json!({"current_turn": current_turn}))
        }
        _ => Err(Error::Domain("Unknown Pi query method".into())),
    }
}
