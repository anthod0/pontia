use super::*;
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
        let service = NativeSessionService::new(events.clone());
        let session = service
            .observed_session("native-test", root.path().to_str().unwrap())
            .await
            .unwrap();
        sqlx::query("INSERT INTO runtime_bindings(session_id,runtime_kind,runtime_instance_id,binding_state) VALUES (?,'external','runtime','confirmed')").bind(&session).execute(&state.db()).await.unwrap();
        events
            .report_fact(ReportedFact {
                session_id: session.clone(),
                turn_id: None,
                fact_type: EventType::SessionReady,
                data: json!({"runtime_instance_id":"runtime"}),
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
    let inbox = crate::InboxCommandService::new(fixture.state.event_ingest_service());
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
            runtime_instance_id: Some("runtime".into()),
        };
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
            .fetch_one(&fixture.state.db())
            .await
            .unwrap();
        if response_first {
            inbox
                .record_receipt(&fixture.session, id, &receipt)
                .await
                .unwrap();
            // Fact ingestion reserves its identity before committing the Turn projection.
            sqlx::query("INSERT INTO native_turn_bindings(session_id,client_turn_id,turn_id) VALUES (?,?,'reserved-turn')")
                .bind(&fixture.session).bind(id).execute(&fixture.state.db()).await.unwrap();
            inbox
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
        fixture
            .service
            .observe_turn(
                &fixture.session,
                "runtime",
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
        inbox
            .record_receipt(&fixture.session, id, &receipt)
            .await
            .unwrap();
        inbox
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
    inbox
        .record_receipt(
            &fixture.session,
            "stale",
            &crate::control::InputReceipt {
                native_turn_id: Some("fact-first".into()),
                runtime_instance_id: Some("old-runtime".into()),
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
            metadata: json!({"launch_cwd":fixture._root.path()}),
        },
        instance_id: "observed-runtime".into(),
        capabilities: crate::views::SessionCapabilities::default(),
        details: json!({"connection":"reconciling"}),
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
    let target =
        crate::runtime::control_target::ControlTarget::resolve(&fixture.state.db(), &session, None)
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
