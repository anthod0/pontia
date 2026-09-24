use crate::{CodexService, rollout::CodexRollout};
use pontia_application::client_contract::{
    TimelineBoundaryBackend, TurnTimelineBackend, TurnTopologyBackend,
};
use pontia_application::{
    EventIngestService,
    clients::{
        BranchTargetRequest, ClientData, ClientOperation, ClientRegistration, ClientSession,
        ClientSessionDetails, NativeEventEvidence,
    },
    control::InputReceipt,
    runtime::ControlTarget,
    sessions::SessionModel,
    turns::InputIntent,
};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};
use pontia_runtime::RuntimeStartRequest;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::{path::Path, sync::Arc};

pub fn registration() -> ClientRegistration {
    ClientRegistration {
        in_process: None,
        spec: &crate::SPEC,
        data: Some(Arc::new(CodexData)),
        launcher: None,
        session: Some(Arc::new(CodexClient)),
        prepare_on_input: true,
        steer: true,
    }
}
struct CodexData;
impl ClientData for CodexData {
    fn normalize_payload(&self, kind: EventType, data: Value) -> Result<Value> {
        if kind.is_turn_event() {
            for field in ["native_turn_id", "runtime_instance_id"] {
                if !data[field].as_str().is_some_and(|value| !value.is_empty()) {
                    return Err(Error::Domain(format!("Codex response missing {field}")));
                }
            }
        }
        Ok(data)
    }
    fn take_evidence(&self, event: &mut DomainEvent) -> NativeEventEvidence {
        NativeEventEvidence {
            entry_anchor: event.payload["native_turn_id"].as_str().map(str::to_owned),
            topology: None,
        }
    }
    fn probe_timeline(
        &self,
        binding: &pontia_application::client_contract::raw_transcripts::AgentBindingResolveRequest,
    ) -> Option<Result<()>> {
        use pontia_application::client_contract::raw_transcripts::AgentBindingResolver;
        Some(CodexRollout.resolve(binding).map(|_| ()))
    }
    fn timeline(&self) -> TurnTimelineBackend {
        TurnTimelineBackend {
            resolver: Box::new(CodexRollout),
            reader: Box::new(CodexRollout),
        }
    }
    fn boundaries(&self) -> TimelineBoundaryBackend {
        TimelineBoundaryBackend {
            resolver: Box::new(CodexRollout),
            capturer: Box::new(CodexRollout),
        }
    }
    fn topology(&self) -> Option<TurnTopologyBackend> {
        None
    }
    fn branch_target(&self, _: BranchTargetRequest) -> Result<String> {
        Err(Error::CapabilityUnavailable(
            "Codex branch control is unsupported".into(),
        ))
    }
}
struct CodexClient;
impl ClientSession for CodexClient {
    fn provision<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let cwd = request
                .workspace
                .as_deref()
                .map(std::path::PathBuf::from)
                .unwrap_or(std::env::current_dir()?);
            CodexService::new(events.clone())
                .provision(&request.session_id, root, &cwd)
                .await?;
            sqlx::query("UPDATE runtime_bindings SET adapter_details=json_set(adapter_details,'$.codex_environment',json(?)) WHERE session_id=?")
                .bind(serde_json::to_string(&request.environment)?).bind(&request.session_id).execute(&events.db()).await?;
            Ok(())
        })
    }
    fn input<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        input: &'a str,
        message: Option<&'a str>,
        intent: &'a InputIntent,
    ) -> ClientOperation<'a, InputReceipt> {
        Box::pin(async move {
            CodexService::new(events)
                .submit(target, input, message, intent)
                .await
        })
    }
    fn interrupt<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        turn: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move { CodexService::new(events).interrupt(target, turn).await })
    }
    fn exit<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move { CodexService::new(events).archive(target).await })
    }
    fn resume<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move { CodexService::new(events).resume(target).await })
    }
    fn list_models<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, Vec<SessionModel>> {
        Box::pin(async move { CodexService::new(events).list_models(target).await })
    }
    fn set_model<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        model: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move { CodexService::new(events).set_model(target, model).await })
    }
    fn available<'a>(&'a self, pool: SqlitePool, session: &'a str) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let state: Option<String> = sqlx::query_scalar("SELECT json_extract(adapter_details,'$.codex.connection') FROM runtime_bindings WHERE session_id=?").bind(session).fetch_optional(&pool).await?.flatten();
            Ok(matches!(
                state.as_deref(),
                Some("available" | "awaiting_input")
            ))
        })
    }
    fn open_interface<'a>(
        &'a self,
        events: EventIngestService,
        session: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move { CodexService::new(events).open_tui(session).await })
    }
    fn details<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async move {
            let details: Option<String> = sqlx::query_scalar("SELECT json_extract(adapter_details,'$.codex') FROM runtime_bindings WHERE session_id=?")
                .bind(session).fetch_optional(&pool).await?.flatten();
            let mut details: serde_json::Value = details
                .map(|value| serde_json::from_str(&value))
                .transpose()?
                .unwrap_or_else(|| serde_json::json!({"connection":"awaiting_input"}));
            let profiles = pontia_application::AgentProfileService::new(pool.clone());
            let profile_status = async {
                let Some(profile) = profiles.codex_binding(session).await? else {
                    return Ok::<_, Error>(Value::Null);
                };
                let bound = pontia_application::AgentBindingService::new(pool.clone())
                    .binding_for_session(session).await?.is_some();
                if bound { profiles.configured_codex_binding(session).await?; }
                Ok(serde_json::json!({"profile_id":profile.profile_id,"version":profile.version,"status":if bound {"configured"} else {"awaiting_input"}}))
            }.await;
            details["profile"] = match profile_status {
                Ok(status) => status,
                Err(error @ (Error::StateConflict(_) | Error::Domain(_))) => {
                    serde_json::json!({"status":"unverified","error":error.to_string()})
                }
                Err(error) => return Err(error),
            };
            let tuis: Vec<CodexTuiView> = sqlx::query_as("SELECT owner_session_id,target_session_id,connected,tmux_socket_path AS socket_path,tmux_pane_id AS pane_id FROM codex_tui_bindings WHERE owner_session_id=? OR target_session_id=? ORDER BY connected DESC, (target_session_id=?) DESC")
                .bind(session).bind(session).bind(session).fetch_all(&pool).await?;
            if let Some(tui) = tuis.iter().find(|tui| tui.owner_session_id == session) {
                details["owned_tui"] = serde_json::to_value(tui)?;
            }
            if let Some(tui) = tuis.first() {
                details["tui"] = serde_json::to_value(tui)?;
            }
            let model_control_unavailable_reason = match details["connection"].as_str() {
                Some("available") => None,
                Some("awaiting_input") => Some(
                    "Send the first message to start this session before choosing a model.".into(),
                ),
                _ => Some("The agent control connection is unavailable.".into()),
            };
            Ok(ClientSessionDetails {
                data: details,
                model_control_unavailable_reason,
            })
        })
    }
}
#[derive(Debug, Serialize, sqlx::FromRow)]
struct CodexTuiView {
    pub owner_session_id: String,
    pub target_session_id: String,
    pub connected: bool,
    pub socket_path: Option<String>,
    pub pane_id: Option<String>,
}
