use crate::client_contract::{
    AgentClientSpec, ClientExitMode, ClientExitOutcome, ClientOperation, ClientSession,
    ClientSessionDetails, DispatchBehavior, RuntimeBehavior, RuntimeBindingBehavior,
    TerminateBehavior, TmuxRuntimeBehavior, TurnLifecycleBehavior,
};
use crate::{
    DeliveryOperation, EventIngestService, InputPayload, control::InputReceipt,
    runtime::ControlTarget,
};
use pontia_runtime::{RuntimeStartRequest, RuntimeStartResult};
use std::path::Path;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub(crate) fn clients() -> super::ClientRegistry {
    clients_with_channel(channel())
}

pub(crate) fn clients_with_channel(channel: Arc<Channel>) -> super::ClientRegistry {
    static SPEC: OnceLock<AgentClientSpec> = OnceLock::new();
    let spec = SPEC.get_or_init(|| {
        let mut spec = crate::client_contract::TEST_SPEC.clone();
        spec.client_type = "test-channel";
        spec.capabilities.interrupt = true;
        spec.adapter.dispatch = DispatchBehavior::Connected;
        spec.adapter.terminate = TerminateBehavior::Connected;
        spec.adapter.turn_lifecycle = TurnLifecycleBehavior::ClientManagedInteractive;
        spec.adapter.runtime = RuntimeBehavior::Tmux(TmuxRuntimeBehavior { hook_log: None });
        spec.adapter.runtime_binding = RuntimeBindingBehavior::Named {
            runtime_kind: "test_tui",
        };
        spec
    });
    let mut registry = super::ClientRegistry::default();
    registry.register(crate::client_contract::test_registration());
    registry.register(super::ClientRegistration {
        configuration: None,
        auth: None,
        interactions: None,
        service: None,
        instructions: None,
        events: None,
        session: Some(channel),
        prepare_on_input: false,
        spec,
        data: None,
    });
    registry
}

#[derive(Default)]
pub(crate) struct Channel {
    pub input: std::sync::Mutex<Vec<String>>,
    pub reasoning_efforts: std::sync::Mutex<Vec<String>>,
    pub closed: AtomicBool,
    pub delayed: bool,
    pub next_error: std::sync::Mutex<Option<pontia_core::Error>>,
    pub started: tokio::sync::Notify,
    pub finish: tokio::sync::Notify,
}
impl Channel {
    async fn await_receipt(&self) {
        if self.delayed {
            self.started.notify_one();
            self.finish.notified().await;
        }
    }
}
impl ClientSession for Channel {
    fn provision<'a>(
        &'a self,
        _events: EventIngestService,
        _root: &'a Path,
        _request: RuntimeStartRequest,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        Box::pin(async { Ok(None) })
    }
    fn set_reasoning_effort<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        reasoning_effort: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            self.reasoning_efforts
                .lock()
                .unwrap()
                .push(reasoning_effort.into());
            Ok(())
        })
    }
    fn steer<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        payload: &'a InputPayload,
    ) -> DeliveryOperation<'a, InputReceipt> {
        self.start_turn(events, target, payload, None)
    }
    fn start_turn<'a>(
        &'a self,
        _events: EventIngestService,
        target: &'a ControlTarget,
        payload: &'a InputPayload,
        _turn: Option<&'a str>,
    ) -> DeliveryOperation<'a, InputReceipt> {
        Box::pin(async move {
            if let Some(error) = self.next_error.lock().unwrap().take() {
                if matches!(error, pontia_core::Error::ControlUnknown(_)) {
                    self.input.lock().unwrap().push(payload.text.clone());
                }
                return Err(error.into());
            }
            self.input.lock().unwrap().push(payload.text.clone());
            self.await_receipt().await;
            Ok(InputReceipt {
                turn_pending: false,
                native_turn_id: None,
                runtime_id: target.runtime_id.clone(),
            })
        })
    }
    fn interrupt<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
    ) -> DeliveryOperation<'a> {
        Box::pin(async { Ok(()) })
    }
    fn exit<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        _mode: ClientExitMode,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async move {
            self.await_receipt().await;
            Ok(ClientExitOutcome::Requested)
        })
    }
    fn available<'a>(
        &'a self,
        _store: crate::ClientControlStore,
        _session: &'a str,
    ) -> ClientOperation<'a, bool> {
        Box::pin(async move { Ok(!self.closed.load(Ordering::SeqCst)) })
    }
    fn details<'a>(
        &'a self,
        _store: crate::ClientControlStore,
        _session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async {
            Ok(ClientSessionDetails {
                data: serde_json::json!({}),
            })
        })
    }
}

pub(crate) fn channel() -> Arc<Channel> {
    Arc::new(Channel::default())
}
