use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use pontia_core::Error;
use pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository;
use tokio::sync::{Notify, mpsc};

use super::manager::{ClientAuthService, Limits};
use crate::{
    client_contract::{
        ClientAuth, ClientOperation,
        auth::{
            AuthContext, AuthErrorCode, AuthEventSink, AuthFlowHandle, AuthFlowUpdate, AuthInput,
            AuthSessionSnapshot, AuthSessionState, AuthState, AuthStep, ClientAuthStatus,
            StartLoginParams,
        },
    },
    clients::ClientRegistry,
    context::ShutdownSignal,
};

const CLIENT: &str = "generic";

#[derive(Default)]
struct Mock {
    state: Mutex<Option<AuthState>>,
    sinks: Mutex<Vec<AuthEventSink>>,
    inputs: Arc<Mutex<Vec<AuthInput>>>,
    cancels: Arc<AtomicUsize>,
    logouts: AtomicUsize,
    probes: AtomicUsize,
    logout_gate: Option<Arc<Notify>>,
}

impl Mock {
    fn set_state(&self, state: AuthState) {
        *self.state.lock().unwrap() = Some(state);
    }

    fn sink(&self) -> AuthEventSink {
        self.sinks.lock().unwrap().last().cloned().unwrap()
    }
}

impl ClientAuth for Mock {
    fn supports_auth(&self, key: &str) -> bool {
        key != "unsupported"
    }

    fn status<'a>(
        &'a self,
        _: &'a str,
        _: &'a AuthContext,
    ) -> ClientOperation<'a, ClientAuthStatus> {
        Box::pin(async move {
            self.probes.fetch_add(1, Ordering::SeqCst);
            let state = self
                .state
                .lock()
                .unwrap()
                .unwrap_or(AuthState::Unauthenticated);
            Ok(ClientAuthStatus::new(state, Vec::new(), true).with_detail("token=secret-value"))
        })
    }

    fn start_login<'a>(
        &'a self,
        _: &'a str,
        method_id: &'a str,
        _: StartLoginParams,
        _: &'a AuthContext,
        sink: AuthEventSink,
    ) -> ClientOperation<'a, AuthFlowHandle> {
        Box::pin(async move {
            if method_id == "bad" {
                return Err(Error::Domain("Unsupported login method".into()));
            }
            let input = if method_id == "input" {
                let (sender, mut receiver) = mpsc::channel(1);
                let inputs = self.inputs.clone();
                tokio::spawn(async move {
                    while let Some(input) = receiver.recv().await {
                        inputs.lock().unwrap().push(input);
                    }
                });
                sink.emit(AuthFlowUpdate::Step {
                    state: AuthSessionState::AwaitingUser,
                    step: AuthStep::AwaitInput {
                        prompt: "Paste".into(),
                        fields: Vec::new(),
                    },
                    expires_in_secs: None,
                })
                .await;
                Some(sender)
            } else {
                sink.emit(AuthFlowUpdate::Step {
                    state: AuthSessionState::AwaitingUser,
                    step: AuthStep::DeviceCode {
                        verification_url: "https://example.test/device".into(),
                        user_code: "ABCD-EFGH".into(),
                        expires_in_secs: Some(120),
                        message: None,
                    },
                    expires_in_secs: Some(120),
                })
                .await;
                None
            };
            self.sinks.lock().unwrap().push(sink);
            let cancels = self.cancels.clone();
            Ok(AuthFlowHandle {
                input,
                cancel: Box::new(move || {
                    Box::pin(async move {
                        cancels.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                }),
            })
        })
    }

    fn logout<'a>(
        &'a self,
        _: &'a str,
        _: Option<&'a str>,
        _: &'a AuthContext,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            if let Some(gate) = &self.logout_gate {
                gate.notified().await;
            }
            self.logouts.fetch_add(1, Ordering::SeqCst);
            self.set_state(AuthState::Unauthenticated);
            Ok(())
        })
    }
}

struct Fixture {
    service: ClientAuthService,
    mock: Arc<Mock>,
    repository: SqliteAgentClientRepository,
    shutdown: ShutdownSignal,
    home: tempfile::TempDir,
}

async fn fixture_with(mock: Mock, limits: Limits) -> Fixture {
    let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    let repository = SqliteAgentClientRepository::new(pool.clone());
    for key in ["default", "other", "unsupported"] {
        repository.save(CLIENT, key, true).await.unwrap();
        repository
            .save_model_catalog(CLIENT, key, None, r#"{"fetched_at":1}"#, 1)
            .await
            .unwrap();
    }
    let mock = Arc::new(mock);
    let mut registration = crate::client_contract::test_registration();
    registration.auth = Some(mock.clone());
    let mut clients = ClientRegistry::default();
    clients.register(registration);
    let shutdown = ShutdownSignal::default();
    let home = tempfile::tempdir().unwrap();
    Fixture {
        service: ClientAuthService::with_limits(
            pool,
            clients,
            home.path().to_path_buf(),
            shutdown.clone(),
            limits,
        ),
        mock,
        repository,
        shutdown,
        home,
    }
}

async fn fixture() -> Fixture {
    fixture_with(Mock::default(), Limits::default()).await
}

fn conflict_code(error: Error) -> &'static str {
    match error {
        Error::Conflict { code, .. } => code,
        other => panic!("expected a conflict, got {other}"),
    }
}

async fn wait_for(
    fixture: &Fixture,
    key: &str,
    id: &str,
    condition: impl Fn(&AuthSessionSnapshot) -> bool,
) -> AuthSessionSnapshot {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(snapshot) = fixture.service.snapshot(CLIENT, key, id)
                && condition(&snapshot)
            {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("login session condition")
}

async fn start(fixture: &Fixture, key: &str) -> AuthSessionSnapshot {
    let snapshot = fixture
        .service
        .start_login(CLIENT, key, "device", StartLoginParams::default())
        .await
        .unwrap();
    wait_for(fixture, key, &snapshot.session_id, |snapshot| {
        snapshot.state == AuthSessionState::AwaitingUser
    })
    .await
}

#[tokio::test]
async fn one_login_runs_per_client_entry_while_entries_run_in_parallel() {
    let fixture = fixture().await;
    let first = start(&fixture, "default").await;
    assert!(first.session_id.starts_with("auth_"));
    assert!(matches!(first.step, AuthStep::DeviceCode { .. }));
    let error = fixture
        .service
        .start_login(CLIENT, "default", "device", StartLoginParams::default())
        .await
        .unwrap_err();
    assert_eq!(conflict_code(error), "auth_session_in_progress");
    let status = fixture.service.status(CLIENT, "default").await.unwrap();
    assert_eq!(
        status.active_session_id.as_deref(),
        Some(first.session_id.as_str())
    );
    assert_eq!(status.detail.as_deref(), Some("token=[REDACTED]"));
    start(&fixture, "other").await;
    let error = fixture
        .service
        .logout(CLIENT, "default", None)
        .await
        .unwrap_err();
    assert_eq!(conflict_code(error), "auth_session_in_progress");
}

#[tokio::test]
async fn status_probe_runs_once_and_persists_the_marker() {
    let fixture = fixture().await;
    fixture.mock.set_state(AuthState::Authenticated);

    let first = fixture.service.probe_statuses_once().await.unwrap();
    assert!(first.first_probe);
    assert_eq!(
        first
            .agent_clients
            .iter()
            .map(|entry| (entry.client_key.as_str(), entry.auth.state))
            .collect::<Vec<_>>(),
        vec![
            ("default", AuthState::Authenticated),
            ("other", AuthState::Authenticated),
            ("unsupported", AuthState::Unsupported),
        ]
    );
    assert_eq!(fixture.mock.probes.load(Ordering::SeqCst), 2);
    assert!(
        fixture
            .home
            .path()
            .join("state/agent-client-auth-probed")
            .is_file()
    );

    let second = fixture.service.probe_statuses_once().await.unwrap();
    assert!(!second.first_probe);
    assert!(second.agent_clients.is_empty());
    assert_eq!(fixture.mock.probes.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn login_is_rejected_while_logout_runs() {
    let gate = Arc::new(Notify::new());
    let fixture = fixture_with(
        Mock {
            logout_gate: Some(gate.clone()),
            ..Mock::default()
        },
        Limits::default(),
    )
    .await;
    let service = fixture.service.clone();
    let logout = tokio::spawn(async move { service.logout(CLIENT, "default", None).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match fixture
                .service
                .start_login(CLIENT, "default", "device", StartLoginParams::default())
                .await
            {
                Err(error) => break assert_eq!(conflict_code(error), "auth_operation_in_progress"),
                Ok(snapshot) => {
                    fixture
                        .service
                        .cancel(CLIENT, "default", &snapshot.session_id)
                        .await
                        .unwrap();
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            }
        }
    })
    .await
    .unwrap();
    gate.notify_one();
    let status = logout.await.unwrap().unwrap();
    assert_eq!(status.state, AuthState::Unauthenticated);
    assert_eq!(
        fixture
            .repository
            .model_catalog(CLIENT, "default")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn login_sessions_are_limited_globally() {
    let fixture = fixture_with(
        Mock::default(),
        Limits {
            max_sessions: 1,
            ..Limits::default()
        },
    )
    .await;
    start(&fixture, "default").await;
    let error = fixture
        .service
        .start_login(CLIENT, "other", "device", StartLoginParams::default())
        .await
        .unwrap_err();
    assert_eq!(conflict_code(error), "auth_capacity_exceeded");
}

#[tokio::test]
async fn completed_login_refreshes_status_and_clears_the_model_catalog() {
    let fixture = fixture().await;
    let session = start(&fixture, "default").await;
    let (current, mut events) = fixture
        .service
        .subscribe(CLIENT, "default", &session.session_id)
        .unwrap();
    assert_eq!(current.sequence, session.sequence);
    fixture.mock.set_state(AuthState::Authenticated);
    assert!(fixture.mock.sink().emit(AuthFlowUpdate::Completed).await);
    let completing = events.recv().await.unwrap();
    assert_eq!(completing.state, AuthSessionState::Completing);
    let terminal = events.recv().await.unwrap();
    assert_eq!(terminal.state, AuthSessionState::Succeeded);
    assert!(terminal.sequence > completing.sequence);
    assert!(terminal.terminal_at.is_some());
    let AuthStep::Succeeded { status } = terminal.step else {
        panic!("expected success");
    };
    assert_eq!(status.state, AuthState::Authenticated);
    assert_eq!(
        fixture
            .repository
            .model_catalog(CLIENT, "default")
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        fixture
            .service
            .status(CLIENT, "default")
            .await
            .unwrap()
            .active_session_id,
        None
    );
    let enabled = fixture
        .repository
        .get(CLIENT, "default")
        .await
        .unwrap()
        .unwrap();
    assert!(enabled.enabled);
}

#[tokio::test]
async fn completion_without_a_credential_fails() {
    let fixture = fixture().await;
    let session = start(&fixture, "default").await;
    fixture.mock.sink().emit(AuthFlowUpdate::Completed).await;
    let settled = wait_for(&fixture, "default", &session.session_id, |snapshot| {
        snapshot.state.is_terminal()
    })
    .await;
    assert_eq!(settled.state, AuthSessionState::Failed);
    assert!(matches!(
        settled.step,
        AuthStep::Failed {
            code: AuthErrorCode::ProviderRejected,
            retryable: true,
            ..
        }
    ));
}

#[tokio::test]
async fn client_failures_are_sanitized() {
    let fixture = fixture().await;
    let session = start(&fixture, "default").await;
    fixture
        .mock
        .sink()
        .emit(AuthFlowUpdate::Failed {
            code: AuthErrorCode::ProviderRejected,
            message: "\u{1b}[31mdenied\u{1b}[0m Bearer abc.def".into(),
            retryable: false,
        })
        .await;
    let settled = wait_for(&fixture, "default", &session.session_id, |snapshot| {
        snapshot.state.is_terminal()
    })
    .await;
    let AuthStep::Failed { message, .. } = settled.step else {
        panic!("expected failure");
    };
    assert_eq!(message, "denied Bearer [REDACTED]");
}

#[tokio::test]
async fn cancellation_is_idempotent_and_scoped_to_the_client_entry() {
    let fixture = fixture().await;
    let session = start(&fixture, "default").await;
    let id = &session.session_id;
    assert!(matches!(
        fixture.service.cancel(CLIENT, "other", id).await,
        Err(Error::NotFound(_))
    ));
    assert!(fixture.service.snapshot(CLIENT, "other", id).is_err());
    let cancelled = fixture.service.cancel(CLIENT, "default", id).await.unwrap();
    assert_eq!(cancelled.state, AuthSessionState::Cancelled);
    assert_eq!(fixture.mock.cancels.load(Ordering::SeqCst), 1);
    let again = fixture.service.cancel(CLIENT, "default", id).await.unwrap();
    assert_eq!(again.sequence, cancelled.sequence);
    assert_eq!(fixture.mock.cancels.load(Ordering::SeqCst), 1);
    let error = fixture
        .service
        .submit_input(CLIENT, "default", id, AuthInput::Confirm)
        .await
        .unwrap_err();
    assert_eq!(conflict_code(error), "auth_session_already_settled");
    fixture.mock.sink().emit(AuthFlowUpdate::Completed).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        fixture
            .service
            .snapshot(CLIENT, "default", id)
            .unwrap()
            .state,
        AuthSessionState::Cancelled
    );

    let failed = start(&fixture, "default").await;
    fixture
        .mock
        .sink()
        .emit(AuthFlowUpdate::Failed {
            code: AuthErrorCode::ProviderRejected,
            message: "denied".into(),
            retryable: true,
        })
        .await;
    wait_for(&fixture, "default", &failed.session_id, |snapshot| {
        snapshot.state.is_terminal()
    })
    .await;
    let error = fixture
        .service
        .cancel(CLIENT, "default", &failed.session_id)
        .await
        .unwrap_err();
    assert_eq!(conflict_code(error), "auth_session_already_settled");
}

#[tokio::test]
async fn input_is_accepted_only_when_the_flow_awaits_it() {
    let fixture = fixture().await;
    let device = start(&fixture, "default").await;
    let error = fixture
        .service
        .submit_input(CLIENT, "default", &device.session_id, AuthInput::Confirm)
        .await
        .unwrap_err();
    assert_eq!(conflict_code(error), "auth_input_not_expected");
    fixture
        .service
        .cancel(CLIENT, "default", &device.session_id)
        .await
        .unwrap();

    let session = fixture
        .service
        .start_login(CLIENT, "default", "input", StartLoginParams::default())
        .await
        .unwrap();
    wait_for(&fixture, "default", &session.session_id, |snapshot| {
        snapshot.state == AuthSessionState::AwaitingUser
    })
    .await;
    fixture
        .service
        .submit_input(CLIENT, "default", &session.session_id, AuthInput::Confirm)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture.mock.inputs.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn failed_start_releases_the_client_entry() {
    let fixture = fixture().await;
    assert!(matches!(
        fixture
            .service
            .start_login(CLIENT, "default", "bad", StartLoginParams::default())
            .await,
        Err(Error::Domain(_))
    ));
    assert_eq!(
        fixture
            .service
            .status(CLIENT, "default")
            .await
            .unwrap()
            .active_session_id,
        None
    );
    start(&fixture, "default").await;
}

#[tokio::test]
async fn unmanaged_and_unknown_entries_are_reported() {
    let fixture = fixture().await;
    let status = fixture.service.status(CLIENT, "unsupported").await.unwrap();
    assert_eq!(status.state, AuthState::Unsupported);
    assert!(status.methods.is_empty() && !status.supports_logout);
    assert!(matches!(
        fixture
            .service
            .start_login(CLIENT, "unsupported", "device", StartLoginParams::default())
            .await,
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        fixture.service.logout(CLIENT, "unsupported", None).await,
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        fixture.service.status(CLIENT, "missing").await,
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        fixture.service.status("missing", "default").await,
        Err(Error::NotFound(_))
    ));
}

#[tokio::test]
async fn expired_sessions_time_out_and_are_evicted() {
    let fixture = fixture_with(
        Mock::default(),
        Limits {
            active_ttl: Duration::from_millis(100),
            terminal_retention: Duration::from_millis(200),
            reaper_interval: Duration::from_millis(20),
            ..Limits::default()
        },
    )
    .await;
    let session = start(&fixture, "default").await;
    let settled = wait_for(&fixture, "default", &session.session_id, |snapshot| {
        snapshot.state.is_terminal()
    })
    .await;
    assert_eq!(settled.state, AuthSessionState::TimedOut);
    assert!(matches!(
        settled.step,
        AuthStep::Failed {
            code: AuthErrorCode::Timeout,
            ..
        }
    ));
    assert_eq!(fixture.mock.cancels.load(Ordering::SeqCst), 1);
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture
            .service
            .snapshot(CLIENT, "default", &session.session_id)
            .is_ok()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    start(&fixture, "default").await;
}

#[tokio::test]
async fn shutdown_cancels_every_login() {
    let fixture = fixture().await;
    let first = start(&fixture, "default").await;
    let second = start(&fixture, "other").await;
    fixture.shutdown.notify();
    for (key, session) in [("default", first), ("other", second)] {
        let settled = wait_for(&fixture, key, &session.session_id, |snapshot| {
            snapshot.state.is_terminal()
        })
        .await;
        assert_eq!(settled.state, AuthSessionState::Cancelled);
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture.mock.cancels.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
