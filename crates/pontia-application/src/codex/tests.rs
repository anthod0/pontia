use super::CodexService;
use crate::{AppState, CreateSessionRequest, SessionCommandService};
use pontia_core::{Error, domain::EventType};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use tokio::sync::broadcast::error::TryRecvError;

struct Fixture {
    state: AppState,
    service: CodexService,
    session: String,
    _root: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("codex.db").display()
        ))
        .await
        .unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::builder(pool, root.path().into()).build();
        let request: CreateSessionRequest =
            serde_json::from_value(json!({"client_type":"codex","workspace":root.path()})).unwrap();
        let created = SessionCommandService::new(state.event_ingest_service(), root.path().into())
            .create_session(request)
            .await
            .unwrap();
        let session = created.session_id().unwrap().to_owned();
        sqlx::query("UPDATE runtime_bindings SET runtime_instance_id='runtime',binding_state='confirmed' WHERE session_id=?")
            .bind(&session).execute(&state.db()).await.unwrap();
        let service = CodexService::new(state.event_ingest_service());
        service
            .report(
                &session,
                "runtime",
                EventType::SessionReady,
                json!({"client_session_key":"thread","launch_cwd":root.path()}),
            )
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
async fn native_turns_publish_committed_facts_without_http() {
    let fixture = Fixture::new().await;
    let ingest = fixture.state.event_ingest_service();
    let mut events = fixture.state.agent_events().subscribe();
    for (native_id, input) in [("manual", "manual input"), ("pontia", "queued input")] {
        if native_id == "pontia" {
            sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary,metadata) VALUES ('message',?,'dispatched','after_idle',?,?)")
                .bind(&fixture.session).bind(input).bind(json!({"codex_turn_id":native_id}).to_string())
                .execute(&fixture.state.db()).await.unwrap();
        }
        let mut turn = json!({
            "id":native_id,"status":"inProgress","items":[
                {"type":"userMessage","content":[{"text":input}]}
            ]
        });
        for observation in ["notification", "snapshot"] {
            fixture
                .service
                .turn_fact(&fixture.session, "runtime", &turn, observation)
                .await
                .unwrap();
        }
        let started = events.try_recv().unwrap();
        assert_eq!(started.event_type, EventType::TurnStarted);
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
        assert_eq!(
            ingest.list_events(&fixture.session).await.unwrap().last(),
            Some(&started)
        );
        let turn_id = started.turn_id.unwrap();
        let projected = ingest.get_turn(&turn_id).await.unwrap().unwrap();
        assert_eq!(projected.state.to_string(), "running");
        assert_eq!(projected.input_summary.as_deref(), Some(input));
        if native_id == "pontia" {
            let linked: Option<String> =
                sqlx::query_scalar("SELECT turn_id FROM inbox_messages WHERE message_id='message'")
                    .fetch_one(&fixture.state.db())
                    .await
                    .unwrap();
            assert_eq!(linked.as_deref(), Some(turn_id.as_str()));
        }
        if native_id == "pontia" {
            fixture
                .service
                .report(
                    &fixture.session,
                    "runtime",
                    EventType::SessionExited,
                    json!({"reason":"thread_archived"}),
                )
                .await
                .unwrap();
            assert_eq!(
                events.try_recv().unwrap().event_type,
                EventType::SessionExited
            );
            assert_eq!(
                ingest
                    .get_turn(&turn_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .state
                    .to_string(),
                "running"
            );
        }
        let terminal = if native_id == "manual" {
            "completed"
        } else {
            "interrupted"
        };
        turn["status"] = json!(terminal);
        turn["items"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"agentMessage","phase":"final_answer","text":"final answer"}));
        for observation in ["snapshot", "notification"] {
            fixture
                .service
                .turn_fact(&fixture.session, "runtime", &turn, observation)
                .await
                .unwrap();
        }
        assert_eq!(events.try_recv().unwrap().event_type, EventType::TurnOutput);
        assert_eq!(
            events.try_recv().unwrap().event_type,
            if terminal == "completed" {
                EventType::TurnCompleted
            } else {
                EventType::TurnInterrupted
            }
        );
        assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
        let projected = ingest.get_turn(&turn_id).await.unwrap().unwrap();
        assert_eq!(projected.state.to_string(), terminal);
        assert_eq!(projected.output_summary.as_deref(), Some("final answer"));
        if native_id == "pontia" {
            assert_eq!(
                ingest
                    .get_session(&fixture.session)
                    .await
                    .unwrap()
                    .unwrap()
                    .state
                    .to_string(),
                "exited"
            );
        }
    }
}

#[tokio::test]
async fn rejected_start_facts_only_publish_failure_for_the_current_runtime() {
    let fixture = Fixture::new().await;
    let mut events = fixture.state.agent_events().subscribe();
    let error = fixture
        .service
        .report(
            &fixture.session,
            "old",
            EventType::TurnStarted,
            json!({"native_turn_id":"old-turn"}),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Domain(_)));
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let ingest = fixture.state.event_ingest_service();
    assert_eq!(
        ingest
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "idle"
    );

    let error = fixture
        .service
        .report(
            &fixture.session,
            "runtime",
            EventType::TurnStarted,
            json!({"native_turn_id":"rejected-turn","metadata":{"oversized":"x".repeat(70_000)}}),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Domain(_)));
    let failure = events.try_recv().unwrap();
    assert_eq!(failure.event_type, EventType::SessionError);
    assert_eq!(failure.payload["reason"], "turn_start_reporting_failed");
    assert_eq!(
        ingest.list_events(&fixture.session).await.unwrap().last(),
        Some(&failure)
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(
        ingest
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "error"
    );
}

#[tokio::test]
async fn storage_failure_keeps_its_error_type_and_does_not_publish_a_fact() {
    let fixture = Fixture::new().await;
    sqlx::query("CREATE TRIGGER reject_started BEFORE INSERT ON events WHEN NEW.event_type='turn.started' BEGIN SELECT RAISE(ABORT, 'test storage failure'); END")
        .execute(&fixture.state.db()).await.unwrap();
    let mut events = fixture.state.agent_events().subscribe();
    let error = fixture
        .service
        .report(
            &fixture.session,
            "runtime",
            EventType::TurnStarted,
            json!({"native_turn_id":"turn"}),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Database(_)));
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let ingest = fixture.state.event_ingest_service();
    assert_eq!(ingest.list_events(&fixture.session).await.unwrap().len(), 2);
    assert_eq!(
        ingest
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "idle"
    );
}
