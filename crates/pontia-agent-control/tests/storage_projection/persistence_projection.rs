use pontia_core::domain::{
    EventSource, EventType, ProjectionState, ReportedEvent, SessionState, TurnState,
};
use serde_json::json;

use crate::fixture::{event, service};

#[tokio::test]
async fn ingest_persists_events_and_updates_projections() {
    let service = service().await;

    service
        .ingest_reported_event(event("evt_1", EventType::SessionCreated, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_reported_event(event("evt_2", EventType::SessionReady, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_3",
            EventType::TurnCreated,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_4",
            EventType::TurnStarted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();
    let result = service
        .ingest_reported_event(event(
            "evt_5",
            EventType::TurnCompleted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();

    assert_eq!(result.state_version, 5);
    assert!(!result.duplicate);

    let session = service.get_session("sess_1").await.unwrap().unwrap();
    let turn = service.get_turn("turn_1").await.unwrap().unwrap();
    let events = service.list_events("sess_1").await.unwrap();

    assert_eq!(session.state, SessionState::Idle);
    assert_eq!(session.current_turn_id.as_deref(), Some("turn_1"));
    assert_eq!(session.state_version, 5);
    assert_eq!(turn.state, TurnState::Completed);
    assert_eq!(events.len(), 5);
}

#[tokio::test]
async fn ingest_persists_turn_input_and_output_summaries() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_summary_session",
            EventType::SessionCreated,
            "sess_summary",
            None,
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(ReportedEvent::new(
            "evt_summary_input".to_string(),
            "sess_summary".to_string(),
            Some("turn_summary".to_string()),
            EventSource::ExternalApi,
            "generic".to_string(),
            EventType::TurnCreated,
            json!({ "input": { "summary": "inspect summaries" } }),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(ReportedEvent::new(
            "evt_summary_output".to_string(),
            "sess_summary".to_string(),
            Some("turn_summary".to_string()),
            EventSource::ExternalApi,
            "generic".to_string(),
            EventType::TurnOutput,
            json!({ "output": { "summary": "summaries persisted" } }),
        ))
        .await
        .unwrap();

    let turn = service.get_turn("turn_summary").await.unwrap().unwrap();
    assert_eq!(turn.input_summary.as_deref(), Some("inspect summaries"));
    assert_eq!(turn.output_summary.as_deref(), Some("summaries persisted"));
}

#[tokio::test]
async fn event_storage_bounds_input_summaries_even_without_http() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_bounded_session",
            EventType::SessionCreated,
            "sess_bounded",
            None,
        ))
        .await
        .unwrap();
    let mut input = event(
        "evt_bounded_input",
        EventType::TurnCreated,
        "sess_bounded",
        Some("turn_bounded"),
    );
    input.payload = json!({ "input": { "summary": "中😀".repeat(1000) } });
    service.ingest_reported_event(input).await.unwrap();
    let events = service.list_events("sess_bounded").await.unwrap();
    assert_eq!(
        events.last().unwrap().payload["input"]["summary"],
        "中😀".repeat(100)
    );
}

#[tokio::test]
async fn session_started_keeps_projection_starting_until_ready() {
    let service = service().await;

    service
        .ingest_reported_event(event(
            "evt_started_created",
            EventType::SessionCreated,
            "sess_started",
            None,
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_started",
            EventType::SessionStarted,
            "sess_started",
            None,
        ))
        .await
        .unwrap();

    let session = service.get_session("sess_started").await.unwrap().unwrap();
    assert_eq!(session.state, SessionState::Starting);
}

#[tokio::test]
async fn storage_rejects_second_active_turn() {
    let service = service().await;

    service
        .ingest_reported_event(event("evt_1", EventType::SessionCreated, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_reported_event(event("evt_2", EventType::SessionReady, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_3",
            EventType::TurnStarted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();

    let result = service
        .ingest_reported_event(event(
            "evt_4",
            EventType::TurnStarted,
            "sess_1",
            Some("turn_2"),
        ))
        .await;

    assert!(result.is_err());
    assert!(service.get_turn("turn_2").await.unwrap().is_none());
    assert_eq!(
        service
            .get_session("sess_1")
            .await
            .unwrap()
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );
}

#[tokio::test]
async fn session_event_turn_context_does_not_create_a_turn() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_session",
            EventType::SessionCreated,
            "sess_1",
            None,
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_session_context",
            EventType::SessionTitleUpdated,
            "sess_1",
            Some("turn_context"),
        ))
        .await
        .unwrap();

    assert!(service.get_turn("turn_context").await.unwrap().is_none());
    let events = service.list_events("sess_1").await.unwrap();
    assert_eq!(events[1].turn_id.as_deref(), Some("turn_context"));
}

#[tokio::test]
async fn replay_preserves_turn_identity_and_state_without_ordinals() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_session",
            EventType::SessionCreated,
            "sess_1",
            None,
        ))
        .await
        .unwrap();

    for (event_id, event_type, turn_id) in [
        ("evt_1", EventType::TurnStarted, "turn_1"),
        ("evt_2", EventType::TurnCompleted, "turn_1"),
        ("evt_3", EventType::TurnStarted, "turn_2"),
        ("evt_4", EventType::TurnCompleted, "turn_2"),
    ] {
        service
            .ingest_reported_event(event(event_id, event_type, "sess_1", Some(turn_id)))
            .await
            .unwrap();
    }

    let events = service.list_events("sess_1").await.unwrap();
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    assert_eq!(replay.turn("turn_1").unwrap().state, TurnState::Completed);
    assert_eq!(replay.turn("turn_2").unwrap().state, TurnState::Completed);
    assert_eq!(replay.turn("turn_1").unwrap().session_id, "sess_1");
    assert_eq!(replay.turn("turn_2").unwrap().session_id, "sess_1");
}

#[tokio::test]
async fn client_title_replaces_first_turn_title_after_projection_reload() {
    for turn_event in [EventType::TurnCreated, EventType::TurnStarted] {
        let service = service().await;
        service
            .ingest_reported_event(event("created", EventType::SessionCreated, "s", None))
            .await
            .unwrap();
        let mut input = event("input", turn_event, "s", Some("t"));
        input.payload = json!({"input":{"summary":"Investigate session naming"}});
        service.ingest_reported_event(input).await.unwrap();
        assert_eq!(
            service
                .get_session("s")
                .await
                .unwrap()
                .unwrap()
                .title
                .as_deref(),
            Some("Investigate session naming")
        );

        let mut update = event("title", EventType::SessionTitleUpdated, "s", None);
        update.source = EventSource::AgentAdapter;
        update.payload = json!({"title":"Fix automatic session titles"});
        service.ingest_reported_event(update).await.unwrap();
        let actual = service.get_session("s").await.unwrap().unwrap();
        assert_eq!(
            actual.title.as_deref(),
            Some("Fix automatic session titles")
        );

        let mut replay = ProjectionState::default();
        for fact in service.list_events("s").await.unwrap() {
            replay.apply(&fact).unwrap();
        }
        let replayed = replay.session("s").unwrap();
        assert_eq!(actual.title, replayed.title);
        assert_eq!(actual.metadata, replayed.metadata);
    }
}

#[tokio::test]
async fn client_titles_respect_manual_names_and_match_history_replay() {
    // (created title, created title_source, [(source, reported title, expected title)])
    type Update = (EventSource, Option<&'static str>, Option<&'static str>);
    let cases: [(Option<&str>, Option<&str>, Vec<Update>); 3] = [
        (
            None,
            None,
            vec![
                (EventSource::AgentClient, Some("Native A"), Some("Native A")),
                (EventSource::AgentClient, None, Some("Native A")),
                (EventSource::ExternalApi, Some("Manual"), Some("Manual")),
                (EventSource::AgentClient, Some("Native B"), Some("Manual")),
                (EventSource::ExternalApi, None, None),
                (EventSource::AgentClient, Some("Native B"), Some("Native B")),
            ],
        ),
        (
            Some("Named at creation"),
            None,
            vec![(
                EventSource::AgentClient,
                Some("Native A"),
                Some("Named at creation"),
            )],
        ),
        (
            Some("Input summary"),
            Some("input"),
            vec![
                (EventSource::AgentClient, None, Some("Input summary")),
                (EventSource::AgentClient, Some("Native A"), Some("Native A")),
            ],
        ),
    ];
    for (initial, title_source, updates) in cases {
        let service = service().await;
        let mut created = event("created", EventType::SessionCreated, "s", None);
        created.payload = json!({"title":initial, "metadata":{"title_source":title_source}});
        service.ingest_reported_event(created).await.unwrap();
        for (index, (source, title, expected)) in updates.into_iter().enumerate() {
            let mut update = event(
                &format!("title-{index}"),
                EventType::SessionTitleUpdated,
                "s",
                None,
            );
            update.source = source;
            update.payload = json!({"title":title});
            service.ingest_reported_event(update).await.unwrap();
            let actual = service.get_session("s").await.unwrap().unwrap();
            assert_eq!(actual.title.as_deref(), expected);
            let mut replay = ProjectionState::default();
            for fact in service.list_events("s").await.unwrap() {
                replay.apply(&fact).unwrap();
            }
            assert_eq!(actual, replay.session("s").unwrap().clone());
        }
    }
}
