use super::{
    AgentBindingService, AppState, EventIngestService, EventSource, EventType, ReportedEvent,
    StatusCode, UpsertAgentBindingRequest, fs, get_json, json, seed_session,
    seed_session_for_client, tempdir, test_state,
};

#[tokio::test]
async fn turn_timeline_returns_empty_for_a_session_without_turns_or_binding() {
    let state = test_state().await;
    let session_id = "sess_empty_turn_timeline";
    seed_session(&state, session_id).await;

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(
        body["data"],
        json!({
            "session_id": session_id,
            "direction": "backward",
            "items": [],
            "next_turn_id": null,
        })
    );

    let (status, history) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/tree/history"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{history:?}");
    assert_eq!(
        history["data"],
        json!({
            "session_id": session_id,
            "groups": [],
            "next_from_turn_id": null,
        })
    );

    let (status, updates) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/tree/updates"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updates:?}");
    assert_eq!(
        updates["data"],
        json!({
            "session_id": session_id,
            "current_turn_id": null,
            "retain_through_turn_id": null,
            "groups": [],
        })
    );
}

#[tokio::test]
async fn turn_timeline_validates_queries_anchors_and_complete_ranges() {
    let state = test_state().await;
    let session_id = "sess_turn_timeline_errors";
    seed_session(&state, session_id).await;

    for query in [
        "",
        "?direction=sideways",
        "?direction=forward&limit=0",
        "?direction=backward&limit=101",
        "?direction=forward&limit=abc",
    ] {
        let (status, body) = get_json(
            state.clone(),
            &format!("/external/v1/sessions/{session_id}/turns/timeline{query}"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(body["error"]["code"], "invalid_timeline_query");
    }

    let (status, body) = get_json(
        state.clone(),
        "/external/v1/sessions/missing/turns/timeline?direction=forward",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body:?}");
    assert_eq!(body["error"]["code"], "session_not_found");

    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            "evt_unsealed_turn".to_string(),
            session_id.to_string(),
            Some("turn_unsealed".to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();
    let (status, body) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id=missing"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_not_found");

    let other_session_id = "sess_turn_timeline_other";
    seed_session(&state, other_session_id).await;
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            "evt_other_session_turn".to_string(),
            other_session_id.to_string(),
            Some("turn_other_session".to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();
    let (status, body) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id=turn_other_session"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_not_found");

    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_unavailable");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("turn_unsealed")
    );
}

#[tokio::test]
async fn turn_timeline_only_allows_the_globally_newest_active_turn() {
    let state = test_state().await;
    let session_id = "sess_open_turn_qualification";
    seed_session(&state, session_id).await;
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, head_cursor, state) VALUES ('turn_01900000-0000-7000-8000-000000000001', ?, 'head', 'running')",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, head_cursor, tail_cursor, state) VALUES ('turn_01900000-0000-7000-8000-000000000002', ?, 'head', 'tail', 'completed')",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();

    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_unavailable");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("turn_01900000-0000-7000-8000-000000000001")
    );
}

#[tokio::test]
async fn turn_timeline_maps_capability_invalid_cursor_and_source_errors() {
    let state = test_state().await;
    for (client_type, session_id, turn_id, expected_status, expected_code) in [
        (
            "generic",
            "sess_turn_timeline_generic",
            "turn_generic",
            StatusCode::UNPROCESSABLE_ENTITY,
            "timeline_capability_unavailable",
        ),
        (
            "claude",
            "sess_turn_timeline_claude",
            "turn_claude",
            StatusCode::SERVICE_UNAVAILABLE,
            "timeline_source_unavailable",
        ),
    ] {
        seed_session_for_client(&state, session_id, client_type).await;
        AgentBindingService::new(state.db())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: session_id.to_string(),
                client_type: client_type.to_string(),
                launch_cwd: "/unused".to_string(),
                client_session_key: format!("{client_type}-timeline"),
                client_session_file: None,
                metadata: json!({}),
            })
            .await
            .unwrap();
        insert_sealed_turn(&state, session_id, turn_id, "head", "tail").await;
        let (status, body) = get_json(
            state.clone(),
            &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
        )
        .await;
        assert_eq!(status, expected_status, "{body:?}");
        assert_eq!(body["error"]["code"], expected_code);
    }

    let temp = tempdir().unwrap();
    let pi_session = "sess_turn_timeline_invalid";
    seed_session(&state, pi_session).await;
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    let source_path = temp.path().join("bound-invalid-cursor.jsonl");
    fs::write(&source_path, b"{\"id\":\"entry\",\"parentId\":null}\n").unwrap();
    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: pi_session.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: "invalid-cursor".to_string(),
            client_session_file: Some(source_path.display().to_string()),
            metadata: json!({}),
        })
        .await
        .unwrap();
    insert_sealed_turn(
        &state,
        pi_session,
        "turn_invalid_cursor",
        &format!("pi-jsonl-v1:{}:0:0", binding.id),
        &format!("pi-jsonl-v2:{}:39:after:entry", binding.id),
    )
    .await;
    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{pi_session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_invalid");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("turn_invalid_cursor")
    );

    let stale_bound_path = temp.path().join("stale-bound-source.jsonl");
    sqlx::query("UPDATE agent_bindings SET client_session_file = ? WHERE id = ?")
        .bind(stale_bound_path.display().to_string())
        .bind(&binding.id)
        .execute(&state.db())
        .await
        .unwrap();
    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{pi_session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body:?}");
    assert_eq!(body["error"]["code"], "timeline_source_unavailable");
    assert!(
        !body
            .to_string()
            .contains(&temp.path().display().to_string())
    );
}

async fn insert_sealed_turn(
    state: &AppState,
    session_id: &str,
    turn_id: &str,
    head_cursor: &str,
    tail_cursor: &str,
) {
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, head_cursor, tail_cursor, state) VALUES (?, ?, ?, ?, 'completed')",
    )
    .bind(turn_id)
    .bind(session_id)
    .bind(head_cursor)
    .bind(tail_cursor)
    .execute(&state.db())
    .await
    .unwrap();
}
