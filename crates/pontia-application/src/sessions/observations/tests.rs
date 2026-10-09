use super::*;
use crate::inbox::InboxAssociations;
use crate::turns::{NativeTurnObservation, NativeTurnService};
use crate::{AppState, ReportedFact};
struct Fixture {
    state: AppState,
    service: NativeSessionService,
    session: String,
    _root: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = pontia_storage_sqlite::connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("test.db").display()
        ))
        .await
        .unwrap();
        pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
        static SPEC: std::sync::OnceLock<crate::client_contract::AgentClientSpec> =
            std::sync::OnceLock::new();
        let spec = SPEC.get_or_init(|| {
            let mut spec = crate::client_contract::TEST_SPEC.clone();
            spec.client_type = "native-test";
            spec.adapter.native_turn_identity = true;
            spec
        });
        let mut clients = crate::clients::ClientRegistry::default();
        let mut registration = crate::client_contract::test_registration();
        registration.spec = spec;
        clients.register(registration);
        let state = AppState::builder(pool, root.path().into())
            .clients(clients)
            .build();
        let events = state.event_ingest_service();
        let service = NativeSessionService::new(state.db(), events.clone());
        let session = service
            .observed_session("native-test", root.path().to_str().unwrap())
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, role, state, created_at) VALUES (?, 'runtime', 'tui', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#)
.bind(&session).execute(&state.db()).await.unwrap();
        events
            .report_fact(ReportedFact {
                session_id: session.clone(),
                turn_id: None,
                fact_type: EventType::SessionReady,
                data: json!({"runtime_id":"runtime"}),
            })
            .await
            .unwrap();
        Self {
            state,
            service,
            session,
            _root: root,
        }
    }
}
#[tokio::test]
async fn input_receipts_link_facts_in_either_order_without_creating_turns() {
    let fixture = Fixture::new().await;
    let inbox = fixture.state.inbox_commands();
    for response_first in [true, false] {
        let id = if response_first {
            "response-first"
        } else {
            "fact-first"
        };
        sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary,metadata) VALUES (?,?,'dispatching','after_idle','input','{}')")
            .bind(id).bind(&fixture.session).execute(&fixture.state.db()).await.unwrap();
        let receipt = crate::control::InputReceipt {
            native_turn_id: Some(id.into()),
            runtime_id: Some("runtime".into()),
        };
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
            .fetch_one(&fixture.state.db())
            .await
            .unwrap();
        if response_first {
            InboxAssociations::new(fixture.state.db(), fixture.state.clients().clone())
                .record_receipt(&fixture.session, id, &receipt)
                .await
                .unwrap();
            // Fact ingestion reserves its identity before committing the Turn projection.
            sqlx::query("INSERT INTO native_turn_bindings(session_id,client_turn_id,turn_id) VALUES (?,?,'reserved-turn')")
                .bind(&fixture.session).bind(id).execute(&fixture.state.db()).await.unwrap();
            InboxAssociations::new(fixture.state.db(), fixture.state.clients().clone())
                .record_receipt(&fixture.session, id, &receipt)
                .await
                .unwrap();
            assert!(
                inbox
                    .get_message(&fixture.session, id)
                    .await
                    .unwrap()
                    .unwrap()
                    .turn_id
                    .is_none()
            );
            let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
                .fetch_one(&fixture.state.db())
                .await
                .unwrap();
            assert_eq!(before, after, "a control reply cannot create a Turn");
        }
        NativeTurnService::new(fixture.state.db(), fixture.state.event_ingest_service())
            .observe_turn(
                &fixture.session,
                Some("runtime"),
                NativeTurnObservation {
                    native_turn_id: id.into(),
                    input_summary: Some("input".into()),
                    output_summary: None,
                    terminal: Ok(Some(EventType::TurnCompleted)),
                    started_at: Value::Null,
                    completed_at: Value::Null,
                    failure: None,
                    origin: "snapshot".into(),
                },
            )
            .await
            .unwrap();
        InboxAssociations::new(fixture.state.db(), fixture.state.clients().clone())
            .record_receipt(&fixture.session, id, &receipt)
            .await
            .unwrap();
        InboxAssociations::new(fixture.state.db(), fixture.state.clients().clone())
            .record_receipt(&fixture.session, id, &receipt)
            .await
            .unwrap();
        let message = inbox
            .get_message(&fixture.session, id)
            .await
            .unwrap()
            .unwrap();
        assert!(message.turn_id.is_some());
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
            .fetch_one(&fixture.state.db())
            .await
            .unwrap();
        assert_eq!(after, before + 1);
    }
    sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary,metadata) VALUES ('stale',?,'dispatching','after_idle','input','{}')")
        .bind(&fixture.session).execute(&fixture.state.db()).await.unwrap();
    InboxAssociations::new(fixture.state.db(), fixture.state.clients().clone())
        .record_receipt(
            &fixture.session,
            "stale",
            &crate::control::InputReceipt {
                native_turn_id: Some("fact-first".into()),
                runtime_id: Some("old-runtime".into()),
            },
        )
        .await
        .unwrap();
    assert!(
        inbox
            .get_message(&fixture.session, "stale")
            .await
            .unwrap()
            .unwrap()
            .turn_id
            .is_none()
    );
}

#[tokio::test]
async fn observed_native_identity_reuses_its_session_without_reprovisioning() {
    let fixture = Fixture::new().await;
    let observation = NativeSessionObservation {
        identity: NativeSessionIdentity {
            launch_cwd: fixture._root.path().display().to_string(),
            client_session_file: None,
        },
        provisioned_runtime: RuntimeStartResult {
            runtime_kind: "external".into(),
            runtime_handle: fixture._root.path().display().to_string(),
            capabilities: crate::views::SessionCapabilities::default(),
            metadata: json!({"launch_cwd":fixture._root.path(),"runtime_id":"observed-runtime"}),
        },
        runtime_id: "observed-runtime".into(),
    };
    let session = fixture
        .service
        .resolve_observed_session("native-test", "thread", Ok(observation))
        .await
        .unwrap();
    let repeated = fixture
        .service
        .resolve_observed_session(
            "native-test",
            "thread",
            Err(Error::Domain(
                "No provisioning metadata in this repeated observation".into(),
            )),
        )
        .await
        .unwrap();
    assert_eq!(session, repeated);
    let binding = crate::AgentBindingService::new(fixture.state.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.client_session_key, "thread");
    let target = crate::runtime::ControlTarget::resolve(&fixture.state.db(), &session, None)
        .await
        .unwrap();
    assert_eq!(target.instance().unwrap(), "observed-runtime");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE session_id != ?")
        .bind(&fixture.session)
        .fetch_one(&fixture.state.db())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

fn binding(fixture: &Fixture) -> UpsertAgentBindingRequest {
    UpsertAgentBindingRequest {
        session_id: fixture.session.clone(),
        client_type: "native-test".into(),
        client_session_key: "thread".into(),
        launch_cwd: fixture._root.path().display().to_string(),
        client_session_file: None,
        metadata: json!({}),
    }
}

async fn instance(fixture: &Fixture) -> String {
    sqlx::query_scalar("SELECT runtime_id FROM session_runtimes WHERE session_id=?")
        .bind(&fixture.session)
        .fetch_one(&fixture.state.db())
        .await
        .unwrap()
}

#[tokio::test]
async fn failed_identity_confirmation_does_not_change_runtime_or_bind_native_identity() {
    let fixture = Fixture::new().await;
    sqlx::query("CREATE TRIGGER reject_binding BEFORE INSERT ON agent_bindings BEGIN SELECT RAISE(ABORT, 'binding write failed'); END")
        .execute(&fixture.state.db()).await.unwrap();
    assert!(
        fixture
            .service
            .confirm(binding(&fixture), "runtime",)
            .await
            .is_err()
    );
    assert_eq!(instance(&fixture).await, "runtime");
    assert!(
        crate::AgentBindingService::new(fixture.state.db())
            .binding_for_session(&fixture.session)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn confirmation_cannot_change_stable_runtime_identity() {
    let fixture = Fixture::new().await;
    let (first, second) = tokio::join!(
        fixture.service.confirm(binding(&fixture), "first",),
        fixture.service.confirm(binding(&fixture), "second",),
    );
    assert!(first.is_err() && second.is_err());
    assert_eq!(instance(&fixture).await, "runtime");
    fixture
        .service
        .confirm(binding(&fixture), "runtime")
        .await
        .unwrap();
    assert_eq!(instance(&fixture).await, "runtime");
}

#[tokio::test]
async fn missing_runtime_cannot_leave_an_agent_binding() {
    let fixture = Fixture::new().await;
    sqlx::query("DELETE FROM session_runtimes WHERE session_id=?")
        .bind(&fixture.session)
        .execute(&fixture.state.db())
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .service
            .confirm(binding(&fixture), "replacement",)
            .await,
        Err(Error::StateConflict(_))
    ));
    assert!(
        crate::AgentBindingService::new(fixture.state.db())
            .binding_for_session(&fixture.session)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn obsolete_ready_cannot_resume_an_exited_session() {
    let fixture = Fixture::new().await;
    fixture
        .service
        .exited(&fixture.session, "runtime", "closed")
        .await
        .unwrap();
    let events = fixture.state.event_ingest_service();
    let before = events.list_events(&fixture.session).await.unwrap().len();
    assert!(
        fixture
            .service
            .ready(&fixture.session, "obsolete", json!({}))
            .await
            .is_err()
    );
    // Even a resume observation that passed an earlier check must be fenced at commit.
    assert!(
        events
            .ingest_runtime_observation_event(PontiaEvent::new(
                &fixture.session,
                None,
                PontiaEventSource::RuntimeManager,
                "native-test",
                PontiaEventType::SessionResuming,
                json!({"runtime_id":"obsolete"})
            ))
            .await
            .is_err()
    );
    assert_eq!(
        events.list_events(&fixture.session).await.unwrap().len(),
        before
    );
    assert_eq!(
        events
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "exited"
    );
    fixture
        .service
        .ready(&fixture.session, "runtime", json!({}))
        .await
        .unwrap();
    assert_eq!(
        events
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "idle"
    );
}

#[tokio::test]
async fn obsolete_turn_observation_and_receipt_cannot_link_inbox() {
    let fixture = Fixture::new().await;
    let turn = || NativeTurnObservation {
        native_turn_id: "native".into(),
        input_summary: None,
        output_summary: None,
        terminal: Ok(None),
        started_at: Value::Null,
        completed_at: Value::Null,
        failure: None,
        origin: "snapshot".into(),
    };
    NativeTurnService::new(fixture.state.db(), fixture.state.event_ingest_service())
        .observe_turn(&fixture.session, Some("runtime"), turn())
        .await
        .unwrap();
    sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary,metadata) VALUES ('pending',?,'dispatching','after_idle','input','{\"native_turn_receipt\":\"native\"}')")
        .bind(&fixture.session).execute(&fixture.state.db()).await.unwrap();
    sqlx::query("UPDATE session_runtimes SET runtime_id='replacement' WHERE session_id=?")
        .bind(&fixture.session)
        .execute(&fixture.state.db())
        .await
        .unwrap();
    let inbox = fixture.state.inbox_commands();
    assert!(
        NativeTurnService::new(fixture.state.db(), fixture.state.event_ingest_service())
            .observe_turn(&fixture.session, Some("runtime"), turn())
            .await
            .is_err()
    );
    InboxAssociations::new(fixture.state.db(), fixture.state.clients().clone())
        .record_receipt(
            &fixture.session,
            "pending",
            &crate::control::InputReceipt {
                native_turn_id: Some("native".into()),
                runtime_id: Some("runtime".into()),
            },
        )
        .await
        .unwrap();
    // Exercise the write-time guard separately from observe_turn's early check.
    InboxAssociations::new(fixture.state.db(), fixture.state.clients().clone())
        .link_native_turn(&fixture.session, "native", Some("runtime"))
        .await
        .unwrap();
    assert!(
        inbox
            .get_message(&fixture.session, "pending")
            .await
            .unwrap()
            .unwrap()
            .turn_id
            .is_none()
    );
    NativeTurnService::new(fixture.state.db(), fixture.state.event_ingest_service())
        .observe_turn(&fixture.session, Some("replacement"), turn())
        .await
        .unwrap();
    assert!(
        inbox
            .get_message(&fixture.session, "pending")
            .await
            .unwrap()
            .unwrap()
            .turn_id
            .is_some()
    );
}

#[tokio::test]
async fn native_terminal_fact_is_fenced_again_when_committing() {
    let fixture = Fixture::new().await;
    let events = fixture.state.event_ingest_service();
    events
        .report_fact(ReportedFact {
            session_id: fixture.session.clone(),
            turn_id: None,
            fact_type: EventType::TurnStarted,
            data: json!({"native_turn_id":"native","runtime_id":"runtime"}),
        })
        .await
        .unwrap();
    let terminal = crate::EventReportNormalizer::new(fixture.state.db())
        .with_clients(events.clients())
        .normalize(ReportedFact {
            session_id: fixture.session.clone(),
            turn_id: None,
            fact_type: EventType::TurnCompleted,
            data: json!({"native_turn_id":"native","runtime_id":"runtime"}),
        })
        .await
        .unwrap();
    let turn = terminal.turn_id.clone().unwrap();
    sqlx::query("UPDATE session_runtimes SET runtime_id='replacement' WHERE session_id=?")
        .bind(&fixture.session)
        .execute(&fixture.state.db())
        .await
        .unwrap();
    let before = events.list_events(&fixture.session).await.unwrap().len();
    assert!(events.ingest_confirmed_event(terminal).await.is_err());
    assert_eq!(
        events
            .get_turn(&turn)
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "running"
    );
    assert_eq!(
        events.list_events(&fixture.session).await.unwrap().len(),
        before
    );
}
