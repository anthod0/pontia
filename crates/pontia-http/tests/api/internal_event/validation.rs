use pontia_application::EventIngestService;
use serde_json::json;

use super::fixture::{bind_runtime, create_session, test_state};
use crate::common::reporting::report_fact_result as report_fact;

#[tokio::test]
async fn reporting_service_rejects_pontia_owned_event_types() {
    let state = test_state().await;
    create_session(&state, "sess_owned_event", "generic").await;

    for fact_type in ["session.created", "turn.dispatch_failed", "turn.abandoned"] {
        let body = report_fact(
            state.clone(),
            json!({
                "session_id": "sess_owned_event",
                "turn_id": "turn_owned_event",
                "type": fact_type,
                "data": {}
            }),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(body, pontia_application::EventReportError::InvalidFact(_)),
            "{body:?}"
        );
        assert!(
            body.to_string()
                .contains("owned by the Pontia control plane"),
            "{body:?}"
        );
    }
}

#[tokio::test]
async fn reporting_service_rejection_does_not_broadcast() {
    let state = test_state().await;
    create_session(&state, "sess_rejected_broadcast", "generic").await;
    let mut subscriber = state.agent_events().subscribe();

    let body = report_fact(
        state.clone(),
        json!({
            "session_id": "sess_rejected_broadcast",
            "type": "turn.completed",
            "data": {}
        }),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(body, pontia_application::EventReportError::InvalidFact(_)),
        "{body:?}"
    );
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn reporting_service_rejects_supplied_unknown_turn_id_for_started_fact() {
    let state = test_state().await;
    create_session(&state, "sess_unknown_started_turn", "pi").await;
    bind_runtime(
        &state,
        "sess_unknown_started_turn",
        "rtinst_unknown_started_turn",
    )
    .await;

    let body = report_fact(
        state,
        json!({
            "session_id": "sess_unknown_started_turn",
            "turn_id": "turn_client_chosen",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_unknown_started_turn" }
        }),
    )
    .await
    .unwrap_err();

    assert!(
        matches!(body, pontia_application::EventReportError::InvalidFact(_)),
        "{body:?}"
    );
}

#[tokio::test]
async fn reporting_service_rejects_other_creation_facts_with_unknown_supplied_turn_ids() {
    let state = test_state().await;
    create_session(&state, "sess_unknown_created_turn", "generic").await;

    for fact_type in ["turn.created", "turn.queued"] {
        let body = report_fact(
            state.clone(),
            json!({
                "session_id": "sess_unknown_created_turn",
                "turn_id": format!("turn_client_chosen_{fact_type}"),
                "type": fact_type,
                "data": {}
            }),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(body, pontia_application::EventReportError::InvalidFact(_)),
            "{body:?}"
        );
    }
}

#[tokio::test]
async fn reporting_service_rejects_unknown_sessions_and_missing_followup_turn_ids() {
    let state = test_state().await;
    let failure = report_fact(
        state.clone(),
        json!({"session_id":"sess_unknown","type":"session.ready","data":{}}),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            failure,
            pontia_application::EventReportError::InvalidFact(_)
        ),
        "{failure:?}"
    );

    create_session(&state, "sess_missing_turn", "generic").await;
    let body = report_fact(
        state,
        json!({"session_id":"sess_missing_turn","type":"turn.output","data":{}}),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(body, pontia_application::EventReportError::InvalidFact(_)),
        "{body:?}"
    );
}

#[tokio::test]
async fn reporting_service_rejects_followups_for_unknown_or_other_session_turns() {
    let state = test_state().await;
    create_session(&state, "sess_turn_owner", "pi").await;
    create_session(&state, "sess_turn_intruder", "pi").await;
    bind_runtime(&state, "sess_turn_owner", "rtinst_owner").await;
    bind_runtime(&state, "sess_turn_intruder", "rtinst_intruder").await;

    let unknown_body = report_fact(
        state.clone(),
        json!({
            "session_id": "sess_turn_owner",
            "turn_id": "turn_missing",
            "type": "turn.completed",
            "data": {}
        }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            unknown_body,
            pontia_application::EventReportError::Ingestion(
                pontia_core::Error::Domain(_) | pontia_core::Error::StateConflict(_)
            )
        ),
        "{unknown_body:?}"
    );

    let started = report_fact(
        state.clone(),
        json!({
            "session_id": "sess_turn_owner",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_owner" }
        }),
    )
    .await
    .unwrap();

    let turn_id = started.turn_id.as_deref().unwrap();
    let cross_session_body = report_fact(
        state,
        json!({
            "session_id": "sess_turn_intruder",
            "turn_id": turn_id,
            "type": "turn.output",
            "data": { "output_summary": "not mine" }
        }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            cross_session_body,
            pontia_application::EventReportError::Ingestion(
                pontia_core::Error::Domain(_) | pontia_core::Error::StateConflict(_)
            )
        ),
        "{cross_session_body:?}"
    );
}

#[tokio::test]
async fn reporting_service_validates_context_usage_and_truncates_output() {
    let state = test_state().await;
    create_session(&state, "sess_validation", "generic").await;

    let failure = report_fact(
        state.clone(),
        json!({
            "session_id":"sess_validation",
            "type":"session.context_usage_updated",
            "data":{"context_usage":{"usage_ratio":2}}
        }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            failure,
            pontia_application::EventReportError::InvalidFact(_)
        ),
        "{failure:?}"
    );

    let started = report_fact(
        state.clone(),
        json!({"session_id":"sess_validation","type":"turn.started","data":{}}),
    )
    .await
    .unwrap();
    let turn_id = started.turn_id.as_deref().expect("turn id");
    report_fact(
        state.clone(),
        json!({
            "session_id":"sess_validation",
            "turn_id":turn_id,
            "type":"turn.output",
            "data":{"output":{"summary":"x".repeat(500)}}
        }),
    )
    .await
    .unwrap();

    let turn = EventIngestService::new(state.db())
        .with_clients(crate::common::clients::clients())
        .get_turn(turn_id)
        .await
        .expect("turn query")
        .expect("turn");
    assert_eq!(turn.output_summary.expect("summary").chars().count(), 200);
}
