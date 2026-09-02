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
use pontia_workflow::{
    AgentEventSubscriber, GracefulExitRequester, SessionCreator, WorkflowCoordinator,
};
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
    pool: sqlx::SqlitePool,
    sender: broadcast::Sender<DomainEvent>,
}

impl TestAgentEvents {
    pub(super) fn new(pool: sqlx::SqlitePool) -> Self {
        Self::with_capacity(pool, 16)
    }

    pub(super) fn with_capacity(pool: sqlx::SqlitePool, capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { pool, sender }
    }

    pub(super) async fn publish(&self, session_id: &str, event_type: EventType) {
        let source = if event_type.is_turn_event() {
            EventSource::AgentAdapter
        } else {
            EventSource::AgentClient
        };
        let event_id = format!("evt_{session_id}_{event_type}");
        let payload = json!({ "runtime_instance_id": format!("runtime_{session_id}") });
        sqlx::query(
            r#"INSERT OR IGNORE INTO events
               (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload)
               VALUES (?, ?, 'turn_root', ?, 'pi', ?, '2026-07-31T00:00:00Z', ?)"#,
        )
        .bind(&event_id)
        .bind(session_id)
        .bind(source.to_string())
        .bind(event_type.to_string())
        .bind(payload.to_string())
        .execute(&self.pool)
        .await
        .expect("persist Agent fact before publishing wake-up hint");
        self.sender
            .send(DomainEvent::new(
                event_id,
                session_id.to_string(),
                Some("turn_root".to_string()),
                source,
                "pi".to_string(),
                event_type,
                payload,
            ))
            .expect("workflow event subscriber");
    }
}

pub(super) fn spawn_coordinator<S, X>(
    pool: sqlx::SqlitePool,
    sessions: S,
    exits: X,
    events: TestAgentEvents,
    pontia_home: std::path::PathBuf,
) -> tokio::task::JoinHandle<()>
where
    S: SessionCreator + Clone + Send + Sync + 'static,
    X: GracefulExitRequester + Clone + Send + Sync + 'static,
{
    let (shutdown_tx, shutdown) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let _keepalive = shutdown_tx;
        WorkflowCoordinator::with_services(pool, sessions, exits, events, pontia_home)
            .run(shutdown)
            .await;
    })
}

impl AgentEventSubscriber for TestAgentEvents {
    fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }
}
