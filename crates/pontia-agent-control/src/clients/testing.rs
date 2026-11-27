use crate::client_contract::{
    AgentClientSpec, DispatchBehavior, RuntimeBehavior, RuntimeBindingBehavior, TerminateBehavior,
    TmuxRuntimeBehavior, TurnLifecycleBehavior,
};
use crate::{ClientControlChannel, ClientControlOperation, DeliveryOperation, InputPayload};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub(crate) fn clients() -> super::ClientRegistry {
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
        in_process: None,
        session: None,
        prepare_on_input: false,
        spec,
        data: None,
        launcher: None,
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
impl ClientControlChannel for Channel {
    fn set_reasoning_effort<'a>(&'a self, reasoning_effort: &'a str) -> ClientControlOperation<'a> {
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
        payload: &'a InputPayload,
        message: Option<&'a str>,
    ) -> DeliveryOperation<'a> {
        self.start_turn(payload, message)
    }
    fn available(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }
    fn invalidate(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
    fn start_turn<'a>(
        &'a self,
        payload: &'a InputPayload,
        _: Option<&'a str>,
    ) -> DeliveryOperation<'a> {
        Box::pin(async move {
            if let Some(error) = self.next_error.lock().unwrap().take() {
                if matches!(error, pontia_core::Error::ControlUnknown(_)) {
                    self.input.lock().unwrap().push(payload.text.clone());
                }
                return Err(error.into());
            }
            self.input.lock().unwrap().push(payload.text.clone());
            self.await_receipt().await;
            Ok(())
        })
    }
    fn interrupt(&self) -> DeliveryOperation<'_> {
        Box::pin(async { Ok(()) })
    }
    fn shutdown(&self) -> ClientControlOperation<'_> {
        Box::pin(async {
            self.await_receipt().await;
            Ok(())
        })
    }
    fn ping(&self) -> ClientControlOperation<'_> {
        Box::pin(async { Ok(()) })
    }
    fn replay<'a>(&'a self, _: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async { Ok(()) })
    }
}

pub(crate) fn channel() -> Arc<Channel> {
    Arc::new(Channel::default())
}
