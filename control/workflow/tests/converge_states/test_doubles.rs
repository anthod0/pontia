use std::{
    collections::VecDeque,
    future::Future,
    sync::{Arc, Mutex},
};

use pontia_application::CreateSessionRequest;
use pontia_core::{
    Error as PontiaError,
    domain::{DomainEvent, EventSource, EventType},
};
use pontia_workflow::{AgentEventSubscriber, GracefulExitRequester, SessionCreator};
use serde_json::json;
use tokio::sync::broadcast;

#[derive(Clone)]
pub(super) struct SequencedSessionCreator {
    outcomes: Arc<Mutex<VecDeque<Option<String>>>>,
    pub(super) requests: Arc<Mutex<Vec<CreateSessionRequest>>>,
}

impl SequencedSessionCreator {
    pub(super) fn new(outcomes: impl IntoIterator<Item = Option<&'static str>>) -> Self {
        Self {
            outcomes: Arc::new(Mutex::new(
                outcomes
                    .into_iter()
                    .map(|outcome| outcome.map(str::to_string))
                    .collect(),
            )),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl SessionCreator for SequencedSessionCreator {
    async fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> pontia_workflow::Result<String> {
        self.requests
            .lock()
            .expect("session requests lock")
            .push(request);
        self.outcomes
            .lock()
            .expect("session outcomes lock")
            .pop_front()
            .expect("configured Session outcome")
            .ok_or_else(|| std::io::Error::other("configured Session creation failure").into())
    }
}

#[derive(Clone, Default)]
pub(super) struct RecordingExitRequester {
    ensure_missing_binding: bool,
    request_error: Arc<Mutex<Option<PontiaError>>>,
    pub(super) requests: Arc<Mutex<Vec<(String, String)>>>,
}

impl RecordingExitRequester {
    pub(super) fn missing_runtime_binding() -> Self {
        Self {
            ensure_missing_binding: true,
            ..Self::default()
        }
    }

    pub(super) fn failing_request() -> Self {
        Self {
            request_error: Arc::new(Mutex::new(Some(PontiaError::Io(std::io::Error::other(
                "configured graceful exit failure",
            ))))),
            ..Self::default()
        }
    }
}

impl GracefulExitRequester for RecordingExitRequester {
    fn ensure_current_runtime(
        &self,
        _session_id: &str,
        _runtime_instance_id: &str,
    ) -> impl Future<Output = pontia_workflow::Result<()>> + Send {
        let missing = self.ensure_missing_binding;
        let session_id = _session_id.to_string();
        async move {
            if missing {
                Err(pontia_workflow::Error::RuntimeControlUnavailable {
                    session_id,
                    message: "no current runtime binding".to_string(),
                })
            } else {
                Ok(())
            }
        }
    }

    fn request_graceful_exit(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = pontia_workflow::Result<()>> + Send {
        self.requests
            .lock()
            .expect("exit requests lock")
            .push((session_id.to_string(), runtime_instance_id.to_string()));
        let error = self
            .request_error
            .lock()
            .expect("request error lock")
            .take();
        async move { error.map_or(Ok(()), |error| Err(error.into())) }
    }
}

#[derive(Clone)]
pub(super) struct TestAgentEvents {
    sender: broadcast::Sender<DomainEvent>,
}

impl TestAgentEvents {
    pub(super) fn new() -> Self {
        Self::with_capacity(16)
    }

    pub(super) fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub(super) fn publish(&self, session_id: &str, event_type: EventType) {
        let source = if event_type.is_turn_event() {
            EventSource::AgentAdapter
        } else {
            EventSource::AgentClient
        };
        self.sender
            .send(DomainEvent::new(
                format!("evt_{event_type}"),
                session_id.to_string(),
                Some("turn_root".to_string()),
                source,
                "pi".to_string(),
                event_type,
                json!({ "runtime_instance_id": format!("runtime_{session_id}") }),
            ))
            .expect("workflow event subscriber");
    }
}

impl AgentEventSubscriber for TestAgentEvents {
    fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }
}
