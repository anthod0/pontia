#[path = "event_reporting/history.rs"]
mod history;
mod support;
use std::time::Duration;

use pontia_application::{
    AgentBindingService, AppState, EventIngestResult, EventReportError, LiveOutputIdentity,
    LiveOutputItem, LiveOutputProducer, LiveOutputSnapshotReplacement, PontiaEvent,
    PontiaEventSource, PontiaEventType, ReportedFact, UpsertAgentBindingRequest,
};
use pontia_core::domain::{EventSource, EventType, TurnTopology};
use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::session_runtimes::{SessionRuntimeRecord, SqliteSessionRuntimeRepository},
    run_migrations,
};
use serde_json::{Value, json};
use tokio::sync::broadcast::error::TryRecvError;

struct Fixture {
    state: AppState,
    root: tempfile::TempDir,
}

impl Fixture {
    async fn new(client: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("events.db").display()
        ))
        .await
        .unwrap();
        run_migrations(&pool).await.unwrap();
        let state = AppState::builder(pool, root.path().into())
            .clients(support::clients())
            .build();
        let fixture = Self { state, root };
        fixture.session("session", client).await;
        fixture.bind("session", "runtime").await;
        fixture
    }

    async fn session(&self, session: &str, client: &str) {
        self.state
            .event_ingest_service()
            .ingest_pontia_event(PontiaEvent::new(
                session,
                None,
                PontiaEventSource::ExternalApi,
                client,
                PontiaEventType::SessionCreated,
                json!({}),
            ))
            .await
            .unwrap();
    }

    async fn bind(&self, session: &str, runtime: &str) {
        SqliteSessionRuntimeRepository::new(self.state.db())
            .upsert_binding(SessionRuntimeRecord {
                session_id: session.into(),
                runtime_id: runtime.into(),
                start_command: None,
                tmux_socket_path: None,
                tmux_pane_id: None,
                process_fingerprint: None,
                role: "tui".into(),
                state: "running".into(),
                created_at: "2026-10-01T00:00:00Z".into(),
            })
            .await
            .unwrap();
    }

    async fn report(
        &self,
        kind: EventType,
        turn: Option<&str>,
        data: Value,
    ) -> Result<EventIngestResult, EventReportError> {
        self.report_with_key(kind, turn, None, data).await
    }

    async fn report_with_key(
        &self,
        kind: EventType,
        turn: Option<&str>,
        report_key: Option<&str>,
        data: Value,
    ) -> Result<EventIngestResult, EventReportError> {
        self.state
            .event_ingest_service()
            .report_fact(ReportedFact {
                session_id: "session".into(),
                turn_id: turn.map(str::to_owned),
                report_key: report_key.map(str::to_owned),
                fact_type: kind,
                data,
            })
            .await
    }

    async fn start(&self) -> String {
        self.report(
            EventType::TurnStarted,
            None,
            json!({"runtime_id":"runtime"}),
        )
        .await
        .unwrap()
        .turn_id
        .unwrap()
    }

    async fn output(&self, turn: &str) {
        self.state
            .live_output()
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: LiveOutputProducer {
                    identity: LiveOutputIdentity {
                        session_id: "session".into(),
                        turn_id: turn.into(),
                        stream_id: "stream".into(),
                    },
                    source: pontia_application::LiveOutputSource::RuntimeBound {
                        runtime_id: "runtime".into(),
                    },
                },
                sequence: 1,
                items: vec![LiveOutputItem::AssistantText {
                    item_id: "item".into(),
                    text: "streamed text".into(),
                }],
            })
            .await
            .unwrap();
        assert!(self.state.live_output().snapshot("session", turn).is_some());
    }
}

#[tokio::test]
async fn normalized_summaries_are_bounded_before_size_validation_and_broadcast_after_commit() {
    let fixture = Fixture::new("pi").await;
    let service = fixture.state.event_ingest_service();
    let mut subscriber = fixture.state.agent_events().subscribe();
    let report = fixture.report(
        EventType::TurnStarted,
        None,
        json!({
            "runtime_id":"runtime", "input_summary":"中😀".repeat(30_000),
            "inbox_message_id":"message", "topology_context":{"kind":"root"},
            "ignored_client_field":"x".repeat(70_000),
        }),
    );
    let observe_commit = async {
        let event = tokio::time::timeout(Duration::from_secs(3), subscriber.recv())
            .await
            .unwrap()
            .unwrap();
        let stored = service.list_events("session").await.unwrap();
        assert_eq!(stored.last().unwrap(), &event);
        assert_eq!(
            service
                .get_turn(event.turn_id.as_deref().unwrap())
                .await
                .unwrap()
                .unwrap()
                .state
                .to_string(),
            "running"
        );
        event
    };
    let (result, event) = tokio::join!(report, observe_commit);
    let result = result.unwrap();
    let turn = result.turn_id.unwrap();
    assert_eq!(event.event_id, result.event_id);
    assert_eq!(event.source, EventSource::AgentAdapter);
    assert_eq!(event.client_type, "pi");
    assert_eq!(event.payload["input"]["summary"], "中😀".repeat(100));
    assert_eq!(event.payload["metadata"]["inbox_message_id"], "message");
    assert_eq!(event.topology, Some(TurnTopology::Unknown));
    assert!(event.payload.get("topology_context").is_none());
    fixture
        .report(
            EventType::TurnOutput,
            Some(&turn),
            json!({"output_summary":"中😀".repeat(30_000)}),
        )
        .await
        .unwrap();
    let projected = service.get_turn(&turn).await.unwrap().unwrap();
    assert_eq!(
        projected.input_summary.as_deref(),
        Some("中😀".repeat(100).as_str())
    );
    assert_eq!(
        projected.output_summary.as_deref(),
        Some("中😀".repeat(100).as_str())
    );
    let events = service.list_events("session").await.unwrap();
    assert_eq!(
        events.last().unwrap().payload["output"]["summary"],
        "中😀".repeat(100)
    );
}

#[tokio::test]
async fn repeated_turn_start_report_returns_the_first_canonical_identity_once() {
    let fixture = Fixture::new("pi").await;
    let mut events = fixture.state.agent_events().subscribe();
    let first = fixture
        .report_with_key(
            EventType::TurnStarted,
            None,
            Some("entry-1"),
            json!({"runtime_id":"runtime","input_summary":"work"}),
        )
        .await
        .unwrap();
    let duplicate = fixture
        .report_with_key(
            EventType::TurnStarted,
            None,
            Some("entry-1"),
            json!({"runtime_id":"runtime","input_summary":"work"}),
        )
        .await
        .unwrap();

    assert!(!first.duplicate);
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.event_id, first.event_id);
    assert_eq!(duplicate.turn_id, first.turn_id);
    assert_eq!(events.recv().await.unwrap().event_id, first.event_id);
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE session_id='session' AND event_type='turn.started'",
    )
    .fetch_one(&fixture.state.db())
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn concurrent_turn_start_reports_create_one_turn() {
    let fixture = Fixture::new("pi").await;
    let service = fixture.state.event_ingest_service();
    let fact = || ReportedFact {
        session_id: "session".into(),
        turn_id: None,
        report_key: Some("entry-concurrent".into()),
        fact_type: EventType::TurnStarted,
        data: json!({"runtime_id":"runtime"}),
    };

    let (left, right) = tokio::join!(service.report_fact(fact()), service.report_fact(fact()));
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.event_id, right.event_id);
    assert_eq!(left.turn_id, right.turn_id);
    assert_ne!(left.duplicate, right.duplicate);
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id='session'")
        .fetch_one(&fixture.state.db())
        .await
        .unwrap();
    assert_eq!(turns, 1);
}

#[tokio::test]
async fn repeated_turn_start_rejects_changed_fact_content() {
    let fixture = Fixture::new("pi").await;
    fixture
        .report_with_key(
            EventType::TurnStarted,
            None,
            Some("entry-1"),
            json!({"runtime_id":"runtime","input_summary":"first","previous_leaf_id":"leaf-1"}),
        )
        .await
        .unwrap();

    for changed in [
        json!({"runtime_id":"runtime","input_summary":"changed","previous_leaf_id":"leaf-1"}),
        json!({"runtime_id":"runtime","input_summary":"first","previous_leaf_id":"leaf-2"}),
    ] {
        let conflict = fixture
            .report_with_key(EventType::TurnStarted, None, Some("entry-1"), changed)
            .await
            .unwrap_err();
        assert!(matches!(
            conflict,
            EventReportError::Ingestion(pontia_core::Error::StateConflict(_))
        ));
    }
}

#[tokio::test]
async fn stale_runtime_cannot_reuse_a_committed_report_key() {
    let fixture = Fixture::new("pi").await;
    fixture
        .report_with_key(
            EventType::TurnStarted,
            None,
            Some("entry-1"),
            json!({"runtime_id":"runtime"}),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE session_runtimes SET runtime_id='replacement' WHERE session_id='session'")
        .execute(&fixture.state.db())
        .await
        .unwrap();

    let stale = fixture
        .report_with_key(
            EventType::TurnStarted,
            None,
            Some("entry-1"),
            json!({"runtime_id":"runtime"}),
        )
        .await
        .unwrap_err();

    assert!(stale.is_permanent_rejection());
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE session_id='session' AND event_type='turn.started'",
    )
    .fetch_one(&fixture.state.db())
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn direct_reports_validate_fact_shape_usage_and_normalized_payload_size() {
    let fixture = Fixture::new("generic").await;
    let mut events = fixture.state.agent_events().subscribe();
    for (kind, data) in [
        (EventType::SessionCreated, json!({})),
        (EventType::TurnStarted, json!(null)),
        (EventType::SessionContextUsageUpdated, json!({})),
        (
            EventType::SessionContextUsageUpdated,
            json!({"context_usage":{"usage_ratio":1.01}}),
        ),
        (
            EventType::SessionContextUsageUpdated,
            json!({"context_usage":{"usage_ratio":-0.01}}),
        ),
        (
            EventType::SessionContextUsageUpdated,
            json!({"context_usage":{"usage_ratio":"0.5"}}),
        ),
        (
            EventType::SessionContextUsageUpdated,
            json!({"context_usage":{"confidence":"certain"}}),
        ),
        (
            EventType::SessionContextUsageUpdated,
            json!({"context_usage":{"model":null}}),
        ),
        (
            EventType::SessionContextUsageUpdated,
            json!({"context_usage":{},"model":4}),
        ),
    ] {
        let error = fixture.report(kind, None, data).await.unwrap_err();
        assert!(
            matches!(error, EventReportError::InvalidFact(_)),
            "{error:?}"
        );
        assert!(error.is_permanent_rejection());
    }
    for field in [
        "used_tokens",
        "max_tokens",
        "remaining_tokens",
        "input_tokens",
        "output_tokens",
        "cache_tokens",
    ] {
        for value in [json!(-1), json!(0.5), json!("1")] {
            let error = fixture
                .report(
                    EventType::SessionContextUsageUpdated,
                    None,
                    json!({"context_usage":{field:value}}),
                )
                .await
                .unwrap_err();
            assert!(matches!(error, EventReportError::InvalidFact(_)));
        }
    }
    assert_eq!(
        fixture
            .state
            .event_ingest_service()
            .list_events("session")
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    fixture
        .report(
            EventType::SessionContextUsageUpdated,
            None,
            json!({"context_usage":{
        "used_tokens":0,"max_tokens":100,"remaining_tokens":100,"input_tokens":null,
        "output_tokens":1,"cache_tokens":0,"usage_ratio":0,"confidence":"exact"
    },"model":"test-model"}),
        )
        .await
        .unwrap();
    let session = fixture
        .state
        .event_ingest_service()
        .get_session("session")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.metadata["model"], "test-model");
}

#[tokio::test]
async fn session_turn_and_ready_identity_boundaries_are_enforced_without_http() {
    let fixture = Fixture::new("pi").await;
    fixture.session("other", "pi").await;
    fixture.bind("other", "other-runtime").await;
    let service = fixture.state.event_ingest_service();
    let error = service
        .report_fact(ReportedFact {
            session_id: "missing".into(),
            turn_id: None,
            report_key: None,
            fact_type: EventType::SessionContextUsageUpdated,
            data: json!({"context_usage":{"used_tokens":1}}),
        })
        .await
        .unwrap_err();
    assert!(matches!(error, EventReportError::InvalidFact(_)));
    for (kind, turn, data) in [
        (EventType::TurnStarted, Some("unknown"), json!({})),
        (EventType::TurnOutput, None, json!({})),
        (
            EventType::SessionReady,
            None,
            json!({"runtime_id":"runtime"}),
        ),
        (
            EventType::SessionReady,
            None,
            json!({"runtime_id":"old","client_session_key":"native"}),
        ),
    ] {
        assert!(matches!(
            fixture.report(kind, turn, data).await.unwrap_err(),
            EventReportError::InvalidFact(_)
        ));
    }
    AgentBindingService::new(fixture.state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: "session".into(),
            client_type: "pi".into(),
            launch_cwd: fixture.root.path().display().to_string(),
            client_session_key: "native".into(),
            client_session_file: None,
            metadata: json!({}),
        })
        .await
        .unwrap();
    let error = fixture
        .report(
            EventType::SessionReady,
            None,
            json!({"runtime_id":"runtime","client_session_key":"different"}),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, EventReportError::InvalidFact(_)));
    let turn = fixture.start().await;
    for (session, turn) in [("session", "unknown"), ("other", turn.as_str())] {
        let error = service
            .report_fact(ReportedFact {
                session_id: session.into(),
                turn_id: Some(turn.into()),
                report_key: None,
                fact_type: EventType::TurnCompleted,
                data: json!({}),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            EventReportError::Ingestion(pontia_core::Error::Domain(_))
        ));
        assert!(error.is_permanent_rejection());
    }
}

#[tokio::test]
async fn rejected_start_does_not_create_lifecycle_facts_or_change_state() {
    let fixture = Fixture::new("pi").await;
    let turn = fixture.start().await;
    fixture.output(&turn).await;
    let mut subscriber = fixture.state.agent_events().subscribe();
    let service = fixture.state.event_ingest_service();
    let event_count = service.list_events("session").await.unwrap().len();

    let error = fixture
        .report(EventType::TurnStarted, None, json!({"runtime_id":"old"}))
        .await
        .unwrap_err();

    assert!(error.is_permanent_rejection());
    assert_eq!(
        service.list_events("session").await.unwrap().len(),
        event_count
    );
    assert_eq!(
        service
            .get_session("session")
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "busy"
    );
    assert!(
        fixture
            .state
            .live_output()
            .snapshot("session", &turn)
            .is_some()
    );
    assert!(matches!(subscriber.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn terminal_reports_clear_shared_live_output() {
    for kind in [
        EventType::TurnCompleted,
        EventType::TurnFailed,
        EventType::TurnInterrupted,
        EventType::SessionExited,
    ] {
        let fixture = Fixture::new("pi").await;
        let turn = fixture.start().await;
        fixture.output(&turn).await;
        fixture
            .report(
                kind,
                kind.is_turn_event().then_some(turn.as_str()),
                json!({"runtime_id":"runtime"}),
            )
            .await
            .unwrap();
        assert!(
            fixture
                .state
                .live_output()
                .snapshot("session", &turn)
                .is_none(),
            "{kind}"
        );
    }
}

#[tokio::test]
async fn failed_transaction_is_not_a_permanent_rejection_or_a_committed_event() {
    let fixture = Fixture::new("pi").await;
    sqlx::query("CREATE TRIGGER reject_started BEFORE INSERT ON events WHEN NEW.event_type = 'turn.started' BEGIN SELECT RAISE(ABORT, 'test storage failure'); END")
        .execute(&fixture.state.db()).await.unwrap();
    let mut subscriber = fixture.state.agent_events().subscribe();
    let error = fixture
        .report(
            EventType::TurnStarted,
            None,
            json!({"runtime_id":"runtime"}),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        EventReportError::Ingestion(pontia_core::Error::Database(_))
    ));
    assert!(!error.is_permanent_rejection());
    assert!(matches!(subscriber.try_recv(), Err(TryRecvError::Empty)));
    let events = fixture
        .state
        .event_ingest_service()
        .list_events("session")
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert!(
        pontia_application::ExternalQueryService::new(fixture.state.db())
            .list_turns("session")
            .await
            .unwrap()
            .is_empty()
    );
}
