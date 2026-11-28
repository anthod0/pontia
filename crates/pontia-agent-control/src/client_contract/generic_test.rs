use std::sync::{Mutex, OnceLock};

use serde_json::json;

use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    ids::new_event_id,
};

use crate::client_contract::{
    AgentClientCapabilities, AgentInput, ContextUsageCapability,
    types::{
        AgentClientAdapter, AgentClientSpec, ClientSessionIdentityBehavior, DispatchBehavior,
        RuntimeBehavior, RuntimeBindingBehavior, TerminateBehavior, TurnLifecycleBehavior,
    },
};

pub const CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
    accept_task: true,
    steer: false,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: false,
    stream_output: false,
    heartbeat: false,
    timeline: false,
    topology: false,
    branch_control: false,
    fork_session: false,
    reasoning_effort_control: false,
    manual_compaction: false,
    compaction_events: false,
    compaction_history: false,
    context_usage: ContextUsageCapability::Unsupported,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "generic",
    default_for_creation: true,
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        lifecycle: super::SessionLifecycleBehavior {
            confirmed_exit_recovery: true,
            ..super::SessionLifecycleBehavior::DEFAULT
        },
        native_turn_identity: false,
        runtime: RuntimeBehavior::InProcess,
        control_runtime_role: Some("generic_control"),
        dispatch: DispatchBehavior::InProcessRecorded,
        client_session_identity: ClientSessionIdentityBehavior::Unsupported,
        terminate: TerminateBehavior::RuntimeManager,
        turn_lifecycle: TurnLifecycleBehavior::BackendManaged,
        runtime_binding: RuntimeBindingBehavior::Unsupported,
    },
};

const REGISTRATION_SPEC: AgentClientSpec = AgentClientSpec {
    capabilities: AgentClientCapabilities {
        accept_task: true,
        steer: true,
        report_turn_started: true,
        report_turn_finished: true,
        interrupt: true,
        stream_output: true,
        heartbeat: true,
        timeline: true,
        topology: true,
        branch_control: true,
        fork_session: true,
        reasoning_effort_control: true,
        manual_compaction: true,
        compaction_events: true,
        compaction_history: true,
        context_usage: ContextUsageCapability::Exact,
    },
    ..SPEC
};

#[derive(Debug, Default, Clone)]
pub struct GenericTestClient;

impl GenericTestClient {
    pub fn clear_recorded_inputs() {
        recorded_inputs()
            .lock()
            .expect("recorded inputs lock")
            .clear();
        *test_capabilities().lock().expect("test capabilities lock") =
            AgentClientCapabilities::generic_default();
    }

    pub fn set_capabilities(capabilities: AgentClientCapabilities) {
        *test_capabilities().lock().expect("test capabilities lock") = capabilities;
    }

    pub fn recorded_inputs() -> Vec<AgentInput> {
        recorded_inputs()
            .lock()
            .expect("recorded inputs lock")
            .clone()
    }

    pub fn ready_event(session_id: &str, runtime_id: &str) -> ReportedEvent {
        ReportedEvent::new(
            new_event_id().to_string(),
            session_id.to_string(),
            None,
            EventSource::AgentClient,
            SPEC.client_type.to_string(),
            EventType::SessionReady,
            json!({ "runtime_id": runtime_id }),
        )
    }
}

impl GenericTestClient {
    pub fn accept_input(&self, input: AgentInput) -> pontia_core::Result<()> {
        recorded_inputs()
            .lock()
            .expect("recorded inputs lock")
            .push(input);
        Ok(())
    }

    pub fn capabilities(&self) -> AgentClientCapabilities {
        test_capabilities()
            .lock()
            .expect("test capabilities lock")
            .clone()
    }
}

fn recorded_inputs() -> &'static Mutex<Vec<AgentInput>> {
    static RECORDED_INPUTS: OnceLock<Mutex<Vec<AgentInput>>> = OnceLock::new();
    RECORDED_INPUTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn test_capabilities() -> &'static Mutex<AgentClientCapabilities> {
    static TEST_CAPABILITIES: OnceLock<Mutex<AgentClientCapabilities>> = OnceLock::new();
    TEST_CAPABILITIES.get_or_init(|| Mutex::new(AgentClientCapabilities::generic_default()))
}

impl crate::client_contract::ClientSession for GenericTestClient {
    fn provision<'a>(
        &'a self,
        _events: crate::EventIngestService,
        root: &'a std::path::Path,
        request: pontia_runtime::RuntimeStartRequest,
    ) -> crate::client_contract::ClientOperation<'a, Option<pontia_runtime::RuntimeStartResult>>
    {
        Box::pin(async move {
            pontia_runtime::GenericRuntimeManager
                .start_in_process(root, request, self.capabilities(), 0)
                .map(Some)
        })
    }
    fn resume_with_runtime<'a>(
        &'a self,
        _events: crate::EventIngestService,
        _target: &'a crate::runtime::ControlTarget,
        request: crate::client_contract::ClientLaunchRequest<'a>,
    ) -> crate::client_contract::ClientOperation<'a, Option<pontia_runtime::RuntimeStartResult>>
    {
        Box::pin(async move {
            pontia_runtime::GenericRuntimeManager
                .start_in_process(
                    request.root,
                    request.runtime,
                    self.capabilities(),
                    request.restart_count,
                )
                .map(Some)
        })
    }
    fn start_turn<'a>(
        &'a self,
        _events: crate::EventIngestService,
        target: &'a crate::runtime::ControlTarget,
        payload: &'a crate::InputPayload,
        _turn: Option<&'a str>,
    ) -> crate::DeliveryOperation<'a, crate::control::InputReceipt> {
        Box::pin(async move {
            self.accept_input(AgentInput {
                session_id: target.session_id.clone(),
                dispatch_id: pontia_core::ids::new_dispatch_id().to_string(),
                input: payload.text.clone(),
            })
            .map_err(|error| match error {
                pontia_core::Error::Domain(message) => crate::DeliveryError::Rejected(message),
                error => crate::DeliveryError::from(error),
            })?;
            Ok(crate::control::InputReceipt {
                turn_pending: false,
                native_turn_id: None,
                runtime_id: target.runtime_id.clone(),
            })
        })
    }
    fn steer<'a>(
        &'a self,
        events: crate::EventIngestService,
        target: &'a crate::runtime::ControlTarget,
        payload: &'a crate::InputPayload,
    ) -> crate::DeliveryOperation<'a, crate::control::InputReceipt> {
        self.start_turn(events, target, payload, None)
    }
    fn exit<'a>(
        &'a self,
        _events: crate::EventIngestService,
        target: &'a crate::runtime::ControlTarget,
        _mode: crate::client_contract::ClientExitMode,
    ) -> crate::client_contract::ClientOperation<'a, crate::client_contract::ClientExitOutcome>
    {
        Box::pin(async move {
            pontia_runtime::GenericRuntimeManager.terminate_session(target.instance()?)?;
            Ok(crate::client_contract::ClientExitOutcome::Requested)
        })
    }
    fn capabilities<'a>(
        &'a self,
        _session: &'a str,
        _implementation: &'a AgentClientCapabilities,
    ) -> crate::client_contract::ClientOperation<'a, AgentClientCapabilities> {
        Box::pin(async move { Ok(self.capabilities()) })
    }
    fn ready(&self, session: &str, instance: &str) -> Option<ReportedEvent> {
        Some(Self::ready_event(session, instance))
    }
    fn available<'a>(
        &'a self,
        _pool: sqlx::SqlitePool,
        _session: &'a str,
    ) -> crate::client_contract::ClientOperation<'a, bool> {
        Box::pin(async { Ok(true) })
    }
    fn details<'a>(
        &'a self,
        _pool: sqlx::SqlitePool,
        _session: &'a str,
    ) -> crate::client_contract::ClientOperation<'a, crate::client_contract::ClientSessionDetails>
    {
        Box::pin(async { Ok(crate::client_contract::ClientSessionDetails { data: json!({}) }) })
    }
}
pub fn registration() -> crate::clients::ClientRegistration {
    crate::clients::ClientRegistration {
        configuration: None,
        auth: None,
        interactions: None,
        service: None,
        instructions: None,
        events: None,
        spec: &REGISTRATION_SPEC,
        data: None,
        session: Some(std::sync::Arc::new(GenericTestClient)),
        prepare_on_input: false,
    }
}
