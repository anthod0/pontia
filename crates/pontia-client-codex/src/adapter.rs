use crate::{
    CodexService,
    rollout::CodexRollout,
    runtime::{CodexRuntime, SubscriptionState},
};
use pontia_application::client_contract::{
    TimelineBoundaryBackend, TurnTimelineBackend, TurnTopologyBackend,
};
use pontia_application::{
    EventIngestService,
    clients::{
        BranchTargetRequest, ClientData, ClientExitOutcome, ClientOperation, ClientRegistration,
        ClientSession, ClientSessionDetails, NativeEventEvidence,
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
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

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
        if kind.is_turn_event()
            && !data["native_turn_id"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        {
            return Err(Error::Domain(
                "Codex response missing native_turn_id".into(),
            ));
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
            CodexService::new(events)
                .provision(&request.session_id, root, &cwd, &request.environment)
                .await
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
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async move { CodexService::new(events).unsubscribe(target).await })
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
            let root: Option<String> = sqlx::query_scalar("SELECT json_extract(metadata,'$.codex_control_root') FROM sessions WHERE session_id=?")
                .bind(session).fetch_optional(&pool).await?.flatten();
            let Some(root) = root else { return Ok(false) };
            let Some(runtime) = CodexRuntime::existing(Path::new(&root)).await else {
                return Ok(false);
            };
            let bound = pontia_application::AgentBindingService::new(pool)
                .binding_for_session(session)
                .await?
                .is_some();
            Ok(if bound {
                matches!(
                    runtime.subscription(session).await,
                    Some(SubscriptionState::Available | SubscriptionState::AwaitingFirstInput)
                )
            } else {
                runtime.connection().await.is_ok()
            })
        })
    }
    fn open_interface<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        session: &'a str,
        runtime_id: &'a str,
    ) -> ClientOperation<'a, RuntimeStartResult> {
        Box::pin(async move {
            let service = CodexService::new(events);
            let binding = pontia_application::AgentBindingService::new(service.pool.clone())
                .binding_for_session(session)
                .await?
                .ok_or_else(|| {
                    Error::StateConflict(
                        "Send the first message before opening the Codex TUI".into(),
                    )
                })?;
            let codex_runtime = service.runtime(session).await?;
            let endpoint = format!("unix://{}", codex_runtime.socket_path.display());
            let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
            let start_command = format!(
                "exec codex resume --remote {} {}",
                quote(&endpoint),
                quote(&binding.client_session_key)
            );
            let mut environment = BTreeMap::new();
            environment.insert(
                "CODEX_HOME".into(),
                codex_runtime.codex_home().display().to_string(),
            );
            let result = GenericRuntimeManager.start_tmux(
                root,
                RuntimeStartRequest {
                    session_id: session.into(),
                    runtime_id: Some(runtime_id.into()),
                    client_type: "codex".into(),
                    workspace: Some(binding.launch_cwd),
                    workspace_name: None,
                    handle: None,
                    role: Some("interface".into()),
                    start_command: Some(start_command),
                    environment,
                },
                1,
                None,
                &crate::SPEC.launch_options(),
            )?;
            let Some(socket) = result.tmux_socket_path() else {
                let _ = GenericRuntimeManager.terminate_session(&result.runtime_handle);
                return Err(Error::Domain(
                    "Codex TUI launch returned no tmux socket".into(),
                ));
            };
            let Some(pane) = result.tmux_pane_id() else {
                let _ = GenericRuntimeManager.terminate_session(&result.runtime_handle);
                return Err(Error::Domain(
                    "Codex TUI launch returned no tmux pane".into(),
                ));
            };
            let fingerprint = match tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Some(fingerprint) = GenericRuntimeManager
                        .capture_tmux_process_fingerprint(socket, pane, &["codex"])
                    {
                        break Ok(fingerprint);
                    }
                    if !GenericRuntimeManager.is_tmux_pane_alive(socket, pane) {
                        break Err(Error::StateConflict(
                            "Codex TUI exited during startup".into(),
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            {
                Ok(Ok(fingerprint)) => fingerprint,
                Ok(Err(error)) => {
                    let _ = GenericRuntimeManager.terminate_session(&result.runtime_handle);
                    return Err(error);
                }
                Err(_) => {
                    let _ = GenericRuntimeManager.terminate_session(&result.runtime_handle);
                    return Err(Error::ControlUnknown(
                        "Codex TUI process was not observable".into(),
                    ));
                }
            };
            let mut result = result;
            result.metadata["tmux_process_fingerprint"] = serde_json::to_value(fingerprint)?;
            Ok(result)
        })
    }
    fn details<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async move {
            let binding = pontia_application::AgentBindingService::new(pool.clone())
                .binding_for_session(session)
                .await?;
            let root: Option<String> = sqlx::query_scalar("SELECT json_extract(metadata,'$.codex_control_root') FROM sessions WHERE session_id=?")
                .bind(session).fetch_optional(&pool).await?.flatten();
            let connection = if let Some(root) = root {
                match CodexRuntime::existing(Path::new(&root)).await {
                    Some(runtime) => match runtime.subscription(session).await {
                        Some(SubscriptionState::Available) => "available",
                        Some(SubscriptionState::AwaitingFirstInput) => "awaiting_input",
                        Some(SubscriptionState::Reconciling) => "reconciling",
                        Some(SubscriptionState::ExitPending) => "unavailable",
                        None if binding.is_none() && runtime.connection().await.is_ok() => {
                            "awaiting_input"
                        }
                        None => "unavailable",
                    },
                    None => "unavailable",
                }
            } else {
                "unavailable"
            };
            let profiles = pontia_application::AgentProfileService::new(pool.clone());
            let profile = match profiles.codex_binding(session).await {
                Ok(Some(profile)) => {
                    let status = if binding.is_some() {
                        match profiles.configured_codex_binding(session).await {
                            Ok(_) => "configured",
                            Err(_) => "unverified",
                        }
                    } else {
                        "awaiting_input"
                    };
                    json!({"profile_id":profile.profile_id,"version":profile.version,"status":status})
                }
                Ok(None) => Value::Null,
                Err(_) => {
                    let selected: Option<(String, String)> = sqlx::query_as("SELECT execution_profile_id, execution_profile_version FROM sessions WHERE session_id=? AND execution_profile_id IS NOT NULL AND execution_profile_version IS NOT NULL")
                        .bind(session).fetch_optional(&pool).await?;
                    selected.map_or(Value::Null, |(profile_id, version)| json!({"profile_id":profile_id,"version":version,"status":"unverified"}))
                }
            };
            let model_control_unavailable_reason = (connection != "available")
                .then(|| "The agent control connection is unavailable.".into());
            Ok(ClientSessionDetails {
                data: json!({"thread_id":binding.map(|binding| binding.client_session_key),"profile":profile,"connection":connection}),
                model_control_unavailable_reason,
            })
        })
    }
}
