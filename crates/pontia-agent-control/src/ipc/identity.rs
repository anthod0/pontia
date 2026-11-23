use super::rpc::RpcRequest;
use crate::{AgentControl, RuntimeBindingUpsertRequest};
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    version: u32,
    binding: RuntimeBindingUpsertRequest,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAttachment {
    pub version: u32,
    pub session_id: String,
    pub runtime_id: String,
    pub client_session_key: String,
}

pub async fn native_registration(
    state: &AgentControl,
    client: &str,
    process_id: Option<u32>,
    request: &RpcRequest,
) -> Result<(NativeAttachment, Value)> {
    match request.method.as_str() {
        "runtime.register" => {
            let registration: Registration = serde_json::from_value(request.params.clone())?;
            if registration.binding.client_type != client {
                return Err(Error::Domain(format!(
                    "Registration requires client_type {client}"
                )));
            }
            let client_session_key = registration.binding.client_session_key.clone();
            let result = state
                .session_runtimes()
                .with_session_identity_hint(client_session_key.clone())
                .upsert(
                    registration.binding,
                    process_id
                        .and_then(pontia_runtime::ProcessFingerprint::capture)
                        .ok_or_else(|| {
                            Error::StateConflict(
                                "Runtime peer process identity is unavailable".into(),
                            )
                        })?,
                )
                .await?;
            let session_id = result["session"]["session_id"]
                .as_str()
                .ok_or_else(|| Error::Domain("Registration returned no session identity".into()))?
                .to_owned();
            let runtime_id = result["runtime"]["runtime_id"]
                .as_str()
                .ok_or_else(|| Error::Domain("Registration returned no runtime identity".into()))?
                .to_owned();
            Ok((
                NativeAttachment {
                    version: registration.version,
                    session_id,
                    runtime_id,
                    client_session_key,
                },
                result,
            ))
        }
        "runtime.attach" => {
            let identity: NativeAttachment = serde_json::from_value(request.params.clone())?;
            let result = json!({"session_id":identity.session_id,"runtime_id":identity.runtime_id});
            Ok((identity, result))
        }
        _ => Err(Error::Domain("Unknown runtime identity method".into())),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeAttachment {
    pub version: u32,
    pub session_id: String,
    pub runtime_id: String,
    pub client_session_key: Option<String>,
}

pub enum BindingConfirmation<'a> {
    Launch,
    Native(&'a str),
}

impl RuntimeAttachment {
    pub fn binding(&self) -> BindingConfirmation<'_> {
        match self.client_session_key.as_deref() {
            Some(key) => BindingConfirmation::Native(key),
            None => BindingConfirmation::Launch,
        }
    }

    pub async fn attach(
        &self,
        state: &AgentControl,
        client: &str,
        channel: std::sync::Arc<dyn crate::ClientControlChannel>,
    ) -> Result<()> {
        let guard = state.client_control().lock_identity().await;
        state
            .client_control()
            .validate_attachment(client, &self.session_id, &self.runtime_id, self.binding())
            .await?;
        let process = channel
            .process_id()
            .and_then(pontia_runtime::ProcessFingerprint::capture)
            .ok_or_else(|| {
                Error::StateConflict("Runtime peer process identity is unavailable".into())
            })?;
        // A provisioned runtime can claim its first process before the client
        // creates a native conversation. Subsequent attachments must match it.
        sqlx::query("UPDATE session_runtimes SET process_fingerprint=? WHERE runtime_id=? AND session_id=? AND state='starting' AND process_fingerprint IS NULL AND EXISTS(SELECT 1 FROM sessions WHERE session_id=? AND client_type=? AND state!='exited')")
            .bind(serde_json::to_string(&process)?).bind(&self.runtime_id).bind(&self.session_id).bind(&self.session_id).bind(client).execute(&state.db()).await?;
        state
            .client_control()
            .attach_binding_locked(
                &guard,
                client,
                &self.session_id,
                &self.runtime_id,
                self.binding(),
                channel,
            )
            .await
    }
}
