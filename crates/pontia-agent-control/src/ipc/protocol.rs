use super::rpc::{PROTOCOL_VERSION, RpcRequest};
use crate::ClientServiceContext;
use pontia_core::Result;
use serde_json::Value;
use std::ops::RangeInclusive;

const SUPPORTED: RangeInclusive<u64> = 8..=PROTOCOL_VERSION as u64;
pub const UNSUPPORTED_VERSION: i64 = -32012;

pub async fn rejection(
    state: &ClientServiceContext,
    process_id: Option<u32>,
    request: &RpcRequest,
) -> Option<String> {
    let message = rejection_message(request)?;
    let result = record_rejection(
        state,
        request,
        process_id,
        None,
        async |session, _runtime| {
            let session_client_type: String =
                sqlx::query_scalar("SELECT client_type FROM sessions WHERE session_id=?")
                    .bind(session)
                    .fetch_one(&state.db())
                    .await?;
            match state
                .clients()
                .get(&session_client_type)
                .and_then(|registration| registration.session.as_ref())
            {
                Some(client) => Ok(client
                    .control_connected(state.store(), session)
                    .await?
                    .unwrap_or(false)),
                None => Ok(false),
            }
        },
    )
    .await;
    if let Err(error) = result {
        tracing::warn!(%error, "Could not record control protocol rejection");
    }
    Some(message)
}

pub fn rejection_message(request: &RpcRequest) -> Option<String> {
    if !matches!(
        request.method.as_str(),
        "runtime.attach" | "runtime.register"
    ) {
        return None;
    }
    let version = request.params.get("version");
    if version
        .and_then(Value::as_u64)
        .is_some_and(|v| SUPPORTED.contains(&v))
    {
        return None;
    }

    let received = version
        .and_then(Value::as_u64)
        .map_or_else(|| "missing or invalid".to_owned(), |v| v.to_string());
    Some(format!(
        "Incompatible control protocol version ({received}). pontiad supports {} through {}. Restart this Session.",
        SUPPORTED.start(),
        SUPPORTED.end()
    ))
}

pub async fn record_rejection(
    state: &ClientServiceContext,
    request: &RpcRequest,
    pid: Option<u32>,
    expected_client: Option<&str>,
    connected: impl AsyncFnOnce(&str, &str) -> Result<bool>,
) -> Result<()> {
    let Some(process) = pid.and_then(pontia_runtime::ProcessFingerprint::capture) else {
        return Ok(());
    };
    let params = &request.params;
    let bound_session;
    let (session, expected_runtime, client_type) = if request.method == "runtime.attach" {
        let (Some(session), Some(runtime)) =
            (params["session_id"].as_str(), params["runtime_id"].as_str())
        else {
            return Ok(());
        };
        (session, Some(runtime), expected_client)
    } else {
        let binding = &params["binding"];
        let (Some(client), Some(key)) = (
            binding["client_type"].as_str(),
            binding["client_session_key"].as_str(),
        ) else {
            return Ok(());
        };
        if expected_client.is_some_and(|expected| expected != client) {
            return Ok(());
        }
        bound_session = crate::AgentBindingService::new(state.db())
            .binding_for_client_session(client, key)
            .await?;
        // Before first registration, a launched Pi uses the Session ID as its native key.
        let session = bound_session
            .as_ref()
            .map_or(key, |b| b.session_id.as_str());
        if binding["session_id"]
            .as_str()
            .is_some_and(|id| id != session)
        {
            return Ok(());
        }
        (session, binding["runtime_id"].as_str(), Some(client))
    };
    let target =
        crate::runtime::ControlTarget::resolve(&state.db(), &state.clients(), session, None)
            .await?;
    let Some(runtime) = target.runtime_id else {
        return Ok(());
    };
    if expected_runtime.is_some_and(|id| id != runtime) || connected(session, &runtime).await? {
        return Ok(());
    }
    // A rejected client stops retrying. Keep its reason across subsequent daemon restarts.
    sqlx::query(
        "UPDATE sessions SET metadata=json_set(metadata,'$.incompatible_control_runtime',?)
         WHERE session_id=? AND (? IS NULL OR client_type=?) AND EXISTS (
             SELECT 1 FROM session_runtimes r WHERE r.session_id=sessions.session_id AND r.runtime_id=?
             AND r.state IN ('starting','running')
             AND (r.process_fingerprint=? OR (r.process_fingerprint IS NULL AND r.state='starting'))
         )",
    )
    .bind(&runtime)
    .bind(session)
    .bind(client_type)
    .bind(client_type)
    .bind(&runtime)
    .bind(serde_json::to_string(&process)?)
    .execute(&state.db())
    .await?;
    Ok(())
}
