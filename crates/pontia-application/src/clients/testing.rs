use crate::{ClientControlChannel, ClientControlOperation};
use pontia_agent_clients::{
    AgentClientSpec, DispatchBehavior, RuntimeBehavior, RuntimeBindingBehavior, TerminateBehavior,
    TmuxRuntimeBehavior, TurnLifecycleBehavior,
};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub(crate) fn clients() -> super::ClientRegistry {
    static SPEC: OnceLock<AgentClientSpec> = OnceLock::new();
    let spec = SPEC.get_or_init(|| {
        let mut spec = pontia_agent_clients::get_client_spec("generic")
            .unwrap()
            .clone();
        spec.client_type = "test-channel";
        spec.capabilities.interrupt = true;
        spec.adapter.dispatch = DispatchBehavior::Connected;
        spec.adapter.terminate = TerminateBehavior::Connected;
        spec.adapter.turn_lifecycle = TurnLifecycleBehavior::ClientManagedForInteractiveTmux;
        spec.adapter.runtime = RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
            process_names: &["test-agent"],
            hook_log: None,
        });
        spec.adapter.runtime_binding = RuntimeBindingBehavior::Tmux {
            runtime_kind: "test_tui",
        };
        spec
    });
    let mut registry = super::ClientRegistry::default();
    registry.register(super::ClientRegistration {
        spec,
        data: None,
        launcher: None,
    });
    registry
}

#[derive(Default)]
pub(crate) struct Channel {
    pub input: std::sync::Mutex<Vec<String>>,
    pub closed: AtomicBool,
    pub delayed: bool,
    pub started: tokio::sync::Notify,
    pub finish: tokio::sync::Notify,
}
impl ClientControlChannel for Channel {
    fn available(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }
    fn invalidate(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
    fn submit<'a>(&'a self, input: &'a str, _: Option<&'a str>) -> ClientControlOperation<'a> {
        Box::pin(async move {
            self.input.lock().unwrap().push(input.to_owned());
            if self.delayed {
                self.started.notify_one();
                self.finish.notified().await;
            }
            Ok(())
        })
    }
    fn list_models(&self) -> ClientControlOperation<'_, Vec<crate::sessions::SessionModel>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn set_model<'a>(&'a self, _: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async { Ok(()) })
    }
    fn interrupt(&self) -> ClientControlOperation<'_> {
        Box::pin(async { Ok(()) })
    }
    fn shutdown(&self) -> ClientControlOperation<'_> {
        Box::pin(async { Ok(()) })
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
