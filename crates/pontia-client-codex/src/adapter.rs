use crate::{
    CodexService,
    rollout::CodexRollout,
    runtime::{CodexRuntime, SubscriptionState},
};
use pontia_agent_control::client_contract::{
    TimelineBoundaryBackend, TurnTimelineBackend, TurnTopologyBackend,
};
use pontia_agent_control::{
    EventIngestService,
    clients::{
        BranchTargetRequest, ClientData, ClientExitMode, ClientExitOutcome, ClientOperation,
        ClientRegistration, ClientSession, ClientSessionDetails, NativeEventEvidence,
    },
    control::InputReceipt,
    runtime::ControlTarget,
    turns::InputIntent,
};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};
use pontia_runtime::RuntimeStartRequest;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{path::Path, sync::Arc};

pub fn registration() -> ClientRegistration {
    ClientRegistration {
        configuration: Some(Arc::new(crate::configuration::CodexConfiguration)),
        interactions: Some(Arc::new(CodexClient)),
        service: Some(Arc::new(crate::service::CodexBackgroundService)),
        instructions: Some(Arc::new(crate::instructions::CodexInstructionsPolicy)),
        events: None,
        in_process: None,
        spec: &crate::SPEC,
        data: Some(Arc::new(CodexData)),
        launcher: None,
        session: Some(Arc::new(CodexClient)),
        prepare_on_input: true,
    }
}

struct CodexData;
impl ClientData for CodexData {
    fn native_history(
        &self,
        binding: pontia_agent_control::client_contract::raw_transcripts::AgentBindingResolveRequest,
        control: Option<pontia_agent_control::clients::ClientControlService>,
    ) -> Option<Arc<dyn pontia_agent_control::client_contract::native_history::NativeHistory>> {
        Some(Arc::new(crate::history::CodexHistory::new(
            binding,
            control.map(|control| control.db()),
        )))
    }
    fn capture_native_boundary(
        &self,
        binding: &pontia_agent_control::client_contract::raw_transcripts::AgentBindingResolveRequest,
        kind: pontia_agent_control::client_contract::raw_transcripts::TimelineBoundaryCaptureKind,
        anchor: Option<String>,
        _first_turn: bool,
        _head: Option<&str>,
    ) -> Option<
        Result<pontia_agent_control::client_contract::raw_transcripts::CapturedTimelineBoundary>,
    > {
        Some(
            anchor
                .ok_or_else(|| Error::Domain("Codex boundary requires native Turn identity".into()))
                .map(|turn| {
                    pontia_agent_control::client_contract::raw_transcripts::CapturedTimelineBoundary {
                        kind,
                        cursor: crate::history::cursor::encode(
                            &binding.id,
                            &binding.client_session_key,
                            &turn,
                            kind,
                        ),
                    }
                }),
        )
    }
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
        _binding: &pontia_agent_control::client_contract::raw_transcripts::AgentBindingResolveRequest,
    ) -> Option<Result<()>> {
        Some(Ok(()))
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
impl pontia_agent_control::client_contract::ClientInteractions for CodexClient {
    fn resolve<'a>(
        &'a self,
        events: EventIngestService,
        session: &'a str,
        interaction: &'a str,
        option: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let service = CodexService::new(events);
            let runtime = CodexRuntime::for_session(&service.pool, session)
                .await?
                .ok_or_else(|| Error::CapabilityUnavailable("Codex connection expired".into()))?;
            let _operation = runtime.lock_session(session).await;
            let live: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id=? AND state<>'exited')",
            )
            .bind(session)
            .fetch_one(&service.pool)
            .await?;
            if !live
                || runtime.subscription(session).await == Some(SubscriptionState::ExitPending)
                || CodexService::has_confirmed_exit(session).await
            {
                return Err(Error::CapabilityUnavailable("Codex Session exited".into()));
            }
            let _current = runtime.current_guard().await?;
            let connection = runtime.connection().await?;
            let binding = service.binding(session).await?;
            if connection
                .unloaded_threads
                .lock()
                .await
                .contains_key(&binding.client_session_key)
            {
                return Err(Error::CapabilityUnavailable(
                    "Codex thread was unloaded".into(),
                ));
            }
            crate::service::interactions::resolve(&connection, session, interaction, option).await
        })
    }
}

impl ClientSession for CodexClient {
    fn fork_session<'a>(
        &'a self,
        events: EventIngestService,
        session: &'a str,
        turn: &'a str,
    ) -> ClientOperation<'a, pontia_agent_control::client_contract::NativeSessionFork> {
        Box::pin(async move { CodexService::new(events).fork_session(session, turn).await })
    }
    fn adopt_fork<'a>(
        &'a self,
        events: EventIngestService,
        session: &'a str,
        fork: &'a pontia_agent_control::client_contract::NativeSessionFork,
    ) -> ClientOperation<'a, String> {
        Box::pin(async move { CodexService::new(events).adopt_fork(session, fork).await })
    }

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
                .provision(
                    &request.session_id,
                    root,
                    &cwd,
                    request.model.as_deref(),
                    request.reasoning_effort.as_deref(),
                    &request.environment,
                )
                .await
        })
    }
    fn input<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        input: &'a str,
        _turn: Option<&'a str>,
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
        turn: Option<&'a str>,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move { CodexService::new(events).interrupt(target, turn).await })
    }
    fn compact<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        operation_id: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            CodexService::new(events)
                .compact(target, operation_id)
                .await
        })
    }
    fn exit<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        mode: ClientExitMode,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async move { CodexService::new(events).exit(target, mode).await })
    }
    fn resume<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move { CodexService::new(events).resume(target).await })
    }
    fn available<'a>(&'a self, pool: SqlitePool, session: &'a str) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let Some(runtime) = CodexRuntime::for_session(&pool, session).await? else {
                return Ok(false);
            };
            let bound = pontia_agent_control::AgentBindingService::new(pool)
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
    fn manual_compaction_available<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let Some(runtime) = CodexRuntime::for_session(&pool, session).await? else {
                return Ok(false);
            };
            Ok(runtime.connection().await.is_ok()
                && runtime.subscription(session).await == Some(SubscriptionState::Available)
                && pontia_agent_control::AgentBindingService::new(pool)
                    .binding_for_session(session)
                    .await?
                    .is_some())
        })
    }
    fn details<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async move {
            let binding = pontia_agent_control::AgentBindingService::new(pool.clone())
                .binding_for_session(session)
                .await?;
            let connection = match CodexRuntime::for_session(&pool, session).await? {
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
            };
            Ok(ClientSessionDetails {
                data: json!({"thread_id":binding.map(|binding| binding.client_session_key),"connection":connection}),
            })
        })
    }
}
