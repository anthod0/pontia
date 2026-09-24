use super::Fixture;
use pontia_application::{
    ExternalQueryService, TurnTimelineDirection, TurnTimelineService, TurnTimelineServiceError,
};
use pontia_core::domain::{EventType, ProjectionState};
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf};

impl Fixture {
    async fn history_file(&self) -> PathBuf {
        let path = self._root.path().join("rollout.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":"thread"}})
            ),
        )
        .unwrap();
        sqlx::query("UPDATE agent_bindings SET client_session_file=? WHERE session_id=?")
            .bind(path.to_str().unwrap())
            .bind(&self.session)
            .execute(&self.state.db())
            .await
            .unwrap();
        path
    }
    async fn native_fact(&self, kind: EventType, native: &str) {
        self.service
            .report(
                &self.session,
                "runtime",
                kind,
                json!({"native_turn_id":native}),
            )
            .await
            .unwrap();
    }
    fn history(&self) -> TurnTimelineService {
        TurnTimelineService::new(self.state.event_ingest_service())
    }
}
fn append(path: &PathBuf, records: &[Value]) {
    let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    for record in records {
        writeln!(file, "{record}").unwrap();
    }
}
fn start(turn: &str) -> Value {
    json!({"type":"event_msg","payload":{"type":"task_started","turn_id":turn}})
}
fn answer(text: &str) -> Value {
    json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":text}]}})
}
fn end(turn: &str) -> Value {
    json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":turn}})
}

#[tokio::test]
async fn native_evidence_captures_readable_new_turns_and_instance_capability() {
    let fixture = Fixture::new().await;
    let path = fixture.history_file().await;
    for native in ["one", "two"] {
        append(&path, &[start(native)]);
        fixture.native_fact(EventType::TurnStarted, native).await;
        append(&path, &[answer(native), end(native)]);
        fixture.native_fact(EventType::TurnCompleted, native).await;
    }
    let query = ExternalQueryService::new(fixture.state.db()).with_clients(fixture.state.clients());
    assert!(
        query
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .timeline
    );
    let page = fixture
        .history()
        .page(
            fixture.session.clone(),
            TurnTimelineDirection::Backward,
            None,
            1,
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].item.content_preview, "two");
    let older = fixture
        .history()
        .page(
            fixture.session.clone(),
            TurnTimelineDirection::Backward,
            page.next_turn_id,
            1,
        )
        .await
        .unwrap();
    assert_eq!(older.items[0].item.content_preview, "one");
    let events = fixture
        .state
        .event_ingest_service()
        .list_events(&fixture.session)
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.timeline_boundary.is_some())
            .count(),
        4
    );
    assert!(
        !events
            .iter()
            .any(|event| event.event_type == EventType::TurnTimelineBoundaryRecovered)
    );
}

#[tokio::test]
async fn delayed_disk_write_recovers_terminal_range_without_changing_lifecycle() {
    let fixture = Fixture::new().await;
    let path = fixture.history_file().await;
    fixture.native_fact(EventType::TurnStarted, "delayed").await;
    fixture.native_fact(EventType::TurnFailed, "delayed").await;
    let ingest = fixture.state.event_ingest_service();
    let events = ingest.list_events(&fixture.session).await.unwrap();
    let turn_id = events.last().unwrap().turn_id.clone().unwrap();
    let before = ingest.get_turn(&turn_id).await.unwrap().unwrap();
    assert!(before.tail_cursor.is_none());
    assert!(matches!(
        fixture
            .history()
            .page(
                fixture.session.clone(),
                TurnTimelineDirection::Backward,
                None,
                20
            )
            .await,
        Err(TurnTimelineServiceError::Pending)
    ));
    append(
        &path,
        &[
            start("delayed"),
            json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"delayed","error":{"message":"native failure details","codex_error_info":"other"}}}),
        ],
    );
    let page = fixture
        .history()
        .page(
            fixture.session.clone(),
            TurnTimelineDirection::Backward,
            None,
            20,
        )
        .await
        .unwrap();
    assert!(
        page.items
            .iter()
            .any(|item| item.item.content_preview.contains("native failure details"))
    );
    let after = ingest.get_turn(&turn_id).await.unwrap().unwrap();
    assert_eq!(after.state, before.state);
    assert_eq!(after.state_version, before.state_version);
    assert_eq!(after.input_summary, before.input_summary);
    assert!(after.tail_cursor.is_some());
    let mut replay = ProjectionState::default();
    for event in ingest.list_events(&fixture.session).await.unwrap() {
        replay.apply(&event).unwrap();
    }
    assert_eq!(replay.turn(&turn_id), Some(&after));
}

#[tokio::test]
async fn old_missing_boundaries_recover_once_even_with_concurrent_readers() {
    let fixture = Fixture::new().await;
    let path = fixture.history_file().await;
    append(
        &path,
        &[start("old"), answer("persisted old answer"), end("old")],
    );
    fixture.native_fact(EventType::TurnStarted, "old").await;
    fixture.native_fact(EventType::TurnCompleted, "old").await;
    // Reproduce the previous adapter's durable result, in this isolated fixture only.
    sqlx::query("UPDATE events SET timeline_boundary=NULL WHERE session_id=?")
        .bind(&fixture.session)
        .execute(&fixture.state.db())
        .await
        .unwrap();
    sqlx::query("UPDATE turns SET head_cursor=NULL,tail_cursor=NULL WHERE session_id=?")
        .bind(&fixture.session)
        .execute(&fixture.state.db())
        .await
        .unwrap();
    let service = fixture.history();
    let (one, two) = tokio::join!(
        service.page(
            fixture.session.clone(),
            TurnTimelineDirection::Backward,
            None,
            20
        ),
        service.page(
            fixture.session.clone(),
            TurnTimelineDirection::Backward,
            None,
            20
        ),
    );
    assert_eq!(one.unwrap().items, two.unwrap().items);
    let events = fixture
        .state
        .event_ingest_service()
        .list_events(&fixture.session)
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == EventType::TurnTimelineBoundaryRecovered)
            .count(),
        2
    );
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    let turn_id = events.last().unwrap().turn_id.as_deref().unwrap();
    assert_eq!(
        replay.turn(turn_id),
        fixture
            .state
            .event_ingest_service()
            .get_turn(turn_id)
            .await
            .unwrap()
            .as_ref()
    );
}

#[tokio::test]
async fn identity_mismatch_never_recovers_or_claims_history_support() {
    let fixture = Fixture::new().await;
    let path = fixture.history_file().await;
    fixture.native_fact(EventType::TurnStarted, "one").await;
    fixture.native_fact(EventType::TurnCompleted, "one").await;
    std::fs::write(
        &path,
        format!(
            "{}\n",
            json!({"type":"session_meta","payload":{"id":"another-thread"}})
        ),
    )
    .unwrap();
    let query = ExternalQueryService::new(fixture.state.db()).with_clients(fixture.state.clients());
    let session = query.get_session(&fixture.session).await.unwrap().unwrap();
    assert!(!session.capabilities.timeline);
    assert!(
        session
            .timeline_unavailable_reason
            .unwrap()
            .contains("different session")
    );
    assert!(matches!(
        fixture
            .history()
            .page(
                fixture.session.clone(),
                TurnTimelineDirection::Backward,
                None,
                20
            )
            .await,
        Err(TurnTimelineServiceError::SourceIdentityMismatch)
    ));
    assert!(
        !fixture
            .state
            .event_ingest_service()
            .list_events(&fixture.session)
            .await
            .unwrap()
            .iter()
            .any(|e| e.event_type == EventType::TurnTimelineBoundaryRecovered)
    );
}

#[tokio::test]
async fn missing_native_association_reports_why_recovery_is_impossible() {
    let fixture = Fixture::new().await;
    let path = fixture.history_file().await;
    fixture.native_fact(EventType::TurnStarted, "unbound").await;
    fixture
        .native_fact(EventType::TurnCompleted, "unbound")
        .await;
    append(
        &path,
        &[start("unbound"), answer("native content"), end("unbound")],
    );
    sqlx::query("DELETE FROM native_turn_bindings WHERE session_id=?")
        .bind(&fixture.session)
        .execute(&fixture.state.db())
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .history()
            .page(
                fixture.session.clone(),
                TurnTimelineDirection::Backward,
                None,
                20
            )
            .await,
        Err(TurnTimelineServiceError::NativeAssociationUnavailable { .. })
    ));
}
