use std::time::Duration;

use pontia_application::{
    AgentBindingService, AppState, EventIngestResult, EventReportError, LiveOutputIdentity,
    LiveOutputItem, LiveOutputProducer, LiveOutputSnapshotReplacement, PontiaEvent,
    PontiaEventSource, PontiaEventType, ReportedFact, UpsertAgentBindingRequest,
};
use pontia_core::domain::{EventSource, EventType, TurnTopology};
use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::runtime_bindings::{RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository},
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
        let state = AppState::builder(pool, root.path().into()).build();
        let fixture = Self { state, root: root };
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
        SqliteRuntimeBindingRepository::new(self.state.db())
            .upsert_binding(RuntimeBindingUpsertRecord {
                session_id: session.into(),
                runtime_kind: "tmux".into(),
                runtime_instance_id: Some(runtime.into()),
                binding_state: "confirmed".into(),
                runtime_handle: None,
                start_command: None,
                launch_cwd: Some(self.root.path().display().to_string()),
                internal_event_url: None,
                started_at: None,
                last_seen_at: None,
                restart_count: 0,
                tmux_socket_path: None,
                tmux_pane_id: None,
                process_fingerprint: None,
                capabilities: "{}".into(),
                diagnostics: "{}".into(),
                adapter_details: "{}".into(),
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
        self.state
            .event_ingest_service()
            .report_fact(ReportedFact {
                session_id: "session".into(),
                turn_id: turn.map(str::to_owned),
                fact_type: kind,
                data,
            })
            .await
    }

    async fn start(&self) -> String {
        self.report(
            EventType::TurnStarted,
            None,
            json!({"runtime_instance_id":"runtime"}),
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
                    runtime_instance_id: "runtime".into(),
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
            "runtime_instance_id":"runtime", "input_summary":"中😀".repeat(30_000),
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
async fn direct_reports_validate_fact_shape_usage_and_normalized_payload_size() {
    let fixture = Fixture::new("generic").await;
    let mut events = fixture.state.agent_events().subscribe();
    for (kind, data) in [
        (EventType::SessionCreated, json!({})),
        (EventType::TurnStarted, json!(null)),
        (EventType::SessionMessageUpdated, json!([1])),
        (
            EventType::SessionMessageUpdated,
            json!({"reason":"x".repeat(65_536)}),
        ),
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
            fact_type: EventType::SessionMessageUpdated,
            data: json!({}),
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
            json!({"runtime_instance_id":"runtime"}),
        ),
        (
            EventType::SessionReady,
            None,
            json!({"runtime_instance_id":"old","client_session_key":"native"}),
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
            json!({"runtime_instance_id":"runtime","client_session_key":"different"}),
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
async fn rejected_start_only_fails_its_current_runtime_and_publishes_the_committed_failure() {
    let fixture = Fixture::new("pi").await;
    let turn = fixture.start().await;
    fixture.output(&turn).await;
    let mut subscriber = fixture.state.agent_events().subscribe();
    let service = fixture.state.event_ingest_service();
    let error = fixture
        .report(
            EventType::TurnStarted,
            None,
            json!({"runtime_instance_id":"old"}),
        )
        .await
        .unwrap_err();
    assert!(error.is_permanent_rejection());
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
    for _ in 0..2 {
        let error = fixture.report(EventType::TurnStarted, None, json!({
            "runtime_instance_id":"runtime", "topology_context":{"oversized":"x".repeat(70_000)}
        })).await.unwrap_err();
        assert!(matches!(error, EventReportError::InvalidFact(_)));
    }
    let event = subscriber.try_recv().unwrap();
    assert_eq!(event.event_type, EventType::SessionError);
    assert_eq!(event.payload["reason"], "turn_start_reporting_failed");
    let stored = service.list_events("session").await.unwrap();
    assert_eq!(stored.last().unwrap(), &event);
    assert_eq!(
        stored
            .iter()
            .filter(|e| e.event_type == EventType::TurnStarted)
            .count(),
        1
    );
    assert!(!stored.iter().any(|e| e.event_type == EventType::TurnFailed));
    assert_eq!(
        service
            .get_session("session")
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "error"
    );
    assert!(matches!(subscriber.try_recv(), Err(TryRecvError::Empty)));
    assert!(
        fixture
            .state
            .live_output()
            .snapshot("session", &turn)
            .is_none()
    );
}

#[tokio::test]
async fn message_refreshes_are_debounced_and_never_persisted() {
    let fixture = Fixture::new("generic").await;
    let mut volatile = fixture.state.volatile_events().subscribe();
    let mut committed = fixture.state.agent_events().subscribe();
    let mut last = None;
    for reason in ["first", "last"] {
        last = Some(
            fixture
                .report(
                    EventType::SessionMessageUpdated,
                    None,
                    json!({"reason":reason}),
                )
                .await
                .unwrap(),
        );
    }
    let event = tokio::time::timeout(Duration::from_secs(3), volatile.recv())
        .await
        .unwrap()
        .unwrap();
    let last = last.unwrap();
    assert_eq!(event.event_id, last.event_id);
    assert_eq!(event.payload["reason"], "last");
    assert_eq!(last.state_version, 1);
    assert!(
        tokio::time::timeout(Duration::from_millis(200), volatile.recv())
            .await
            .is_err()
    );
    assert!(matches!(committed.try_recv(), Err(TryRecvError::Empty)));
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
                json!({"runtime_instance_id":"runtime"}),
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
async fn concurrent_native_facts_create_one_turn_and_one_committed_notification() {
    let fixture = Fixture::new("codex").await;
    let service = fixture.state.event_ingest_service();
    let mut subscriber = fixture.state.agent_events().subscribe();
    let data = json!({"runtime_instance_id":"runtime","native_turn_id":"native-one","input":{"summary":"manual input"}});
    let (first, second) = tokio::join!(
        fixture.report(EventType::TurnStarted, None, data.clone()),
        fixture.report(EventType::TurnStarted, None, data),
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(first.turn_id, second.turn_id);
    assert_eq!(first.event_id, second.event_id);
    assert_ne!(first.duplicate, second.duplicate);
    assert_eq!(subscriber.try_recv().unwrap().event_id, first.event_id);
    assert!(matches!(subscriber.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(service.list_events("session").await.unwrap().len(), 2);
    fixture.bind("session", "replacement").await;
    for kind in [EventType::TurnStarted, EventType::TurnCompleted] {
        let error = fixture
            .report(
                kind,
                None,
                json!({"runtime_instance_id":"runtime","native_turn_id":"native-one"}),
            )
            .await
            .unwrap_err();
        assert!(error.is_permanent_rejection());
    }
    assert!(matches!(subscriber.try_recv(), Err(TryRecvError::Empty)));
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
    fixture
        .report(
            EventType::SessionExited,
            None,
            json!({"runtime_instance_id":"replacement","reason":"thread_archived"}),
        )
        .await
        .unwrap();
    let turn_id = first.turn_id.unwrap();
    assert_eq!(
        service
            .get_turn(&turn_id)
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "running"
    );
    fixture.report(EventType::TurnInterrupted, None, json!({"runtime_instance_id":"replacement","native_turn_id":"native-one","native_completed_at":null,"observation":"snapshot"})).await.unwrap();
    let query = pontia_application::ExternalQueryService::new(fixture.state.db());
    let turns = query.list_turns("session").await.unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].state, "interrupted");
    assert_eq!(turns[0].completed_at, None);
    assert_eq!(
        query.get_session("session").await.unwrap().unwrap().state,
        "exited"
    );
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
            json!({"runtime_instance_id":"runtime"}),
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
