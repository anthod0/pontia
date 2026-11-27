use super::*;
use pontia_agent_control::{EventReportError, ReportedFact};
use pontia_core::domain::EventType;

#[tokio::test]
async fn repeated_observations_have_distinct_keys_and_each_report_replays_once() {
    let h = Harness::new(&[("ACP_MODELS", "flat"), ("ACP_LIVE_SCRIPT", "1")]).await;
    let usage = json!({"update":{"sessionUpdate":"usage_update","used":10,"size":20}});
    let model = json!({"update":{"sessionUpdate":"config_option_update","configOptions":[{
        "id":"llm-selector","name":"Model","category":"model","type":"select",
        "currentValue":"deep","options":[{"name":"Deep","value":"deep"}]
    }]}});
    std::fs::write(
        h.root.path().join("live-script.json"),
        json!([[usage, model, usage]]).to_string(),
    )
    .unwrap();
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .start_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.release("live-1-0");
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();

    let events = h.app.event_ingest_service();
    let history = events.list_events(&session).await.unwrap();
    let mut keys = std::collections::HashSet::new();
    for (kind, count) in [
        (EventType::SessionReady, 1),
        (EventType::SessionModelUpdated, 2),
        (EventType::SessionContextUsageUpdated, 2),
        (EventType::TurnStarted, 1),
        (EventType::TurnCompleted, 1),
        (EventType::SessionExited, 1),
    ] {
        let observations: Vec<_> = history.iter().filter(|e| e.event_type == kind).collect();
        assert_eq!(observations.len(), count, "{kind}");
        for observed in observations {
            let key = observed.report_key.as_ref().unwrap();
            assert!(
                keys.insert((kind.to_string(), key)),
                "independent {kind} observations must have distinct keys"
            );
            let fact = ReportedFact {
                session_id: session.clone(),
                turn_id: observed.turn_id.clone(),
                report_key: Some(key.clone()),
                fact_type: kind,
                data: observed.payload.clone(),
            };
            let replay = events.report_fact(fact.clone()).await.unwrap();
            assert!(replay.accepted && replay.duplicate);
            assert_eq!(replay.event_id, observed.event_id);
            assert_eq!(replay.turn_id, observed.turn_id);
            let mut changed = fact;
            changed.data["different"] = json!(true);
            assert!(matches!(
                events.report_fact(changed).await,
                Err(EventReportError::Ingestion(Error::StateConflict(_)))
            ));
        }
    }
    assert_eq!(
        events.list_events(&session).await.unwrap().len(),
        history.len()
    );
    h.stop().await;
}

#[tokio::test]
async fn keyed_start_requires_the_same_preallocated_turn_in_its_session() {
    let h = Harness::new(&[("ACP_UNIQUE_ID", "1")]).await;
    let session = h.create("first").await.unwrap();
    let mut turns = Vec::new();
    for index in 1..=2 {
        let turn = h
            .app
            .turn_commands()
            .start_turn(&session, "end_turn".into(), json!({}))
            .await
            .unwrap()
            .unwrap();
        h.release(&format!("update-{index}"));
        h.release(&format!("release-{index}"));
        h.wait_turn(&session, &turn.turn_id, "completed").await;
        turns.push(turn.turn_id);
    }
    let events = h.app.event_ingest_service();
    let start = events
        .list_events(&session)
        .await
        .unwrap()
        .into_iter()
        .find(|e| e.event_type == EventType::TurnStarted)
        .unwrap();
    let mut fact = ReportedFact {
        session_id: session.clone(),
        turn_id: Some(turns[1].clone()),
        report_key: start.report_key,
        fact_type: EventType::TurnStarted,
        data: start.payload,
    };
    assert!(matches!(
        events.report_fact(fact.clone()).await,
        Err(EventReportError::Ingestion(Error::StateConflict(_)))
    ));
    fact.turn_id = Some("missing".into());
    assert!(matches!(
        events.report_fact(fact.clone()).await,
        Err(EventReportError::InvalidFact(_))
    ));
    fact.session_id = h.create("second").await.unwrap();
    fact.turn_id = Some(turns[0].clone());
    assert!(matches!(
        events.report_fact(fact).await,
        Err(EventReportError::InvalidFact(_))
    ));
    assert_eq!(h.events(&session, "turn.started").await, 2);
    h.stop().await;
}

#[tokio::test]
async fn each_close_of_a_resumed_session_records_its_own_exit() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    let commands = h.app.session_commands();
    commands.terminate_session(&session).await.unwrap();
    commands
        .resume_session(&session, h.root.path())
        .await
        .unwrap();
    commands.terminate_session(&session).await.unwrap();
    let exits: Vec<String> = sqlx::query_scalar(
        "SELECT report_key FROM events WHERE session_id=? AND event_type='session.exited' ORDER BY rowid"
    ).bind(&session).fetch_all(&h.app.db()).await.unwrap();
    assert_eq!(exits.len(), 2);
    assert_ne!(exits[0], exits[1]);
    h.wait_session_state(&session, "exited").await;
    h.stop().await;
}
