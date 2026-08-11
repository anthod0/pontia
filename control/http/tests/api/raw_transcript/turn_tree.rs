use super::*;

#[tokio::test]
async fn pi_hook_context_projects_a_replayable_conversation_tree_without_persisting_native_evidence()
 {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_pi_linear_topology";
    let session_key = "pi-linear-topology";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;
    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let transcript = session_dir.join(format!("2026-07-16T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(&transcript, b"").unwrap();
    AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: session_key.to_string(),
            client_session_file: Some(transcript.display().to_string()),
            metadata: json!({}),
        })
        .await
        .unwrap();

    let turns = [
        (
            "turn_pi_linear_1",
            "evt_pi_linear_1",
            Value::Array(vec![]),
            None,
        ),
        (
            "turn_pi_linear_2",
            "evt_pi_linear_2",
            json!([
                {"id": "user_1", "kind": "user_message"},
                {"id": "assistant_1", "kind": "assistant_message"}
            ]),
            Some("assistant_1"),
        ),
        (
            "turn_pi_linear_3",
            "evt_pi_linear_3",
            json!([
                {"id": "user_1", "kind": "user_message"},
                {"id": "assistant_1", "kind": "assistant_message"},
                {"id": "model_2", "kind": "model_change"},
                {"id": "user_2", "kind": "user_message"},
                {"id": "assistant_2", "kind": "assistant_message"}
            ]),
            Some("assistant_2"),
        ),
    ];

    for (index, (turn_id, _event_prefix, entries, previous_leaf_id)) in turns.iter().enumerate() {
        precreate_turn_if_missing(&state, session_id, turn_id).await;
        let started = json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_pi_linear",
                "timeline_anchor": { "previous_leaf_id": previous_leaf_id },
                "topology_context": { "entries": entries },
            }
        });
        let (status, body) = post_internal_event(state.clone(), started.clone()).await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
        let user_id = format!("user_{}", index + 1);
        let assistant_id = format!("assistant_{}", index + 1);
        fs::write(
            &transcript,
            (1..=index + 1)
                .map(|number| {
                    let user_id = format!("user_{number}");
                    let assistant_id = format!("assistant_{number}");
                    let parent_id = (number > 1).then(|| format!("assistant_{}", number - 1));
                    pi_text_turn_entries(
                        &user_id,
                        parent_id.as_deref(),
                        &format!("question {number}"),
                        &assistant_id,
                        &format!("answer {number}"),
                    )
                })
                .collect::<String>(),
        )
        .unwrap();
        let completed = json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.completed",
            "data": {
                "runtime_instance_id": "rtinst_pi_linear",
                "timeline_anchor": { "terminal_leaf_id": assistant_id },
                "debug_content": user_id,
            }
        });
        assert_eq!(
            post_internal_event(state.clone(), completed).await.0,
            StatusCode::OK
        );
    }

    let branch_turns = [
        (
            "turn_pi_linear_4",
            "evt_pi_linear_4",
            json!([
                {"id": "user_1", "kind": "user_message"},
                {"id": "assistant_1", "kind": "assistant_message"}
            ]),
            "assistant_1",
            "assistant_1",
            "user_4",
            "assistant_4",
        ),
        (
            "turn_pi_linear_5",
            "evt_pi_linear_5",
            json!([
                {"id": "user_1", "kind": "user_message"},
                {"id": "assistant_1", "kind": "assistant_message"},
                {"id": "user_4", "kind": "user_message"},
                {"id": "assistant_4", "kind": "assistant_message"}
            ]),
            "assistant_4",
            "assistant_4",
            "user_5",
            "assistant_5",
        ),
    ];
    for (
        turn_id,
        _event_prefix,
        entries,
        previous_leaf_id,
        native_parent_id,
        user_id,
        assistant_id,
    ) in branch_turns
    {
        precreate_turn_if_missing(&state, session_id, turn_id).await;
        let started = json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_pi_linear",
                "timeline_anchor": { "previous_leaf_id": previous_leaf_id },
                "topology_context": { "entries": entries },
            }
        });
        let (status, body) = post_internal_event(state.clone(), started).await;
        assert_eq!(status, StatusCode::OK, "{body:?}");

        let mut transcript_file = fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .unwrap();
        transcript_file
            .write_all(
                pi_text_turn_entries(
                    user_id,
                    Some(native_parent_id),
                    &format!("question {user_id}"),
                    assistant_id,
                    &format!("answer {assistant_id}"),
                )
                .as_bytes(),
            )
            .unwrap();
        let completed = json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.completed",
            "data": {
                "runtime_instance_id": "rtinst_pi_linear",
                "timeline_anchor": { "terminal_leaf_id": assistant_id },
            }
        });
        assert_eq!(
            post_internal_event(state.clone(), completed).await.0,
            StatusCode::OK
        );
    }

    let (status, initial_history) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/tree/history?limit=5"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{initial_history:?}");
    assert_eq!(
        initial_history["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["turn_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["turn_pi_linear_1", "turn_pi_linear_4", "turn_pi_linear_5"]
    );

    let (status, history) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/history?from_turn_id=turn_pi_linear_5&limit=2"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{history:?}");
    assert_eq!(
        history["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["turn_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["turn_pi_linear_4", "turn_pi_linear_5"]
    );
    assert_eq!(history["data"]["next_from_turn_id"], "turn_pi_linear_1");

    let (status, older_history) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/history?from_turn_id=turn_pi_linear_1&limit=2"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{older_history:?}");
    assert_eq!(
        older_history["data"]["groups"][0]["turn_id"],
        "turn_pi_linear_1"
    );
    assert!(older_history["data"]["next_from_turn_id"].is_null());

    let (status, updates) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_pi_linear_3"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updates:?}");
    assert_eq!(updates["data"]["current_turn_id"], "turn_pi_linear_5");
    assert_eq!(
        updates["data"]["retain_through_turn_id"],
        "turn_pi_linear_1"
    );
    assert_eq!(
        updates["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["turn_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["turn_pi_linear_4", "turn_pi_linear_5"]
    );

    let (status, inclusive_updates) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_pi_linear_4"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{inclusive_updates:?}");
    assert_eq!(
        inclusive_updates["data"]["retain_through_turn_id"],
        "turn_pi_linear_4"
    );
    assert_eq!(
        inclusive_updates["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["turn_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["turn_pi_linear_4", "turn_pi_linear_5"]
    );

    precreate_turn_if_missing(&state, session_id, "turn_pi_linear_6").await;
    let disconnected_started = json!({
        "session_id": session_id,
        "turn_id": "turn_pi_linear_6",
        "type": "turn.started",
        "data": {
            "runtime_instance_id": "rtinst_pi_linear",
            "timeline_anchor": { "previous_leaf_id": "assistant_5" },
            "topology_context": { "entries": [] },
        }
    });
    assert_eq!(
        post_internal_event(state.clone(), disconnected_started)
            .await
            .0,
        StatusCode::OK
    );
    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            pi_text_turn_entries(
                "user_6",
                Some("assistant_5"),
                "disconnected question",
                "assistant_6",
                "disconnected answer",
            )
            .as_bytes(),
        )
        .unwrap();
    let disconnected_completed = json!({
        "session_id": session_id,
        "turn_id": "turn_pi_linear_6",
        "type": "turn.completed",
        "data": {
            "runtime_instance_id": "rtinst_pi_linear",
            "timeline_anchor": { "terminal_leaf_id": "assistant_6" },
        }
    });
    assert_eq!(
        post_internal_event(state.clone(), disconnected_completed)
            .await
            .0,
        StatusCode::OK
    );

    let (status, disconnected_updates) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_pi_linear_5"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{disconnected_updates:?}");
    assert!(disconnected_updates["data"]["retain_through_turn_id"].is_null());
    assert_eq!(
        disconnected_updates["data"]["groups"][0]["turn_id"],
        "turn_pi_linear_6"
    );

    precreate_turn_if_missing(&state, session_id, "turn_pi_linear_malformed").await;
    let malformed_started = json!({
        "session_id": session_id,
        "turn_id": "turn_pi_linear_malformed",
        "type": "turn.started",
        "data": {
            "runtime_instance_id": "rtinst_pi_linear",
            "timeline_anchor": { "previous_leaf_id": "assistant_6" },
            "topology_context": { "entries": [
                {"id": "native-secret-entry", "kind": "user_message"},
                {"id": "native-secret-entry", "kind": "assistant_message"}
            ] },
        }
    });
    let captured_logs = CapturedLogWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_writer(captured_logs.clone())
        .finish();
    let (malformed_status, malformed_body) = post_internal_event(state.clone(), malformed_started)
        .with_subscriber(subscriber)
        .await;
    assert_eq!(malformed_status, StatusCode::OK, "{malformed_body:?}");
    let log_text = captured_logs.text();
    let warning = log_text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|entry| entry["fields"]["code"] == "turn_topology_unresolved")
        .expect("structured topology warning");
    assert_eq!(warning["fields"]["diagnostic"], "evidence_invalid");
    assert!(!log_text.contains("native-secret-entry"));
    let turn_five = EventIngestService::new(state.db())
        .get_turn("turn_pi_linear_5")
        .await
        .unwrap()
        .unwrap();
    assert!(!log_text.contains(turn_five.tail_cursor.as_deref().unwrap()));

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let projected = body["data"]["turns"].as_array().unwrap();
    assert_eq!(projected.len(), 7);
    assert!(
        projected
            .iter()
            .all(|turn| turn.get("turn_index").is_none())
    );
    assert_eq!(projected[0]["topology_status"], "root");
    assert_eq!(projected[1]["parent_turn_id"], "turn_pi_linear_1");
    assert_eq!(projected[2]["parent_turn_id"], "turn_pi_linear_2");
    assert_eq!(projected[3]["parent_turn_id"], "turn_pi_linear_1");
    assert_eq!(projected[4]["parent_turn_id"], "turn_pi_linear_4");
    assert_eq!(projected[5]["topology_status"], "root");
    assert_eq!(projected[6]["topology_status"], "unknown");
    assert_eq!(projected[6]["state"], "running");

    let (status, unknown_updates) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_pi_linear_5"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{unknown_updates:?}");
    assert_eq!(unknown_updates["error"]["code"], "turn_topology_unknown");

    for (selected_turn_id, expected_preview) in [
        ("turn_pi_linear_3", "question 3"),
        ("turn_pi_linear_4", "question user_4"),
    ] {
        let (status, selected) = get_json(
            state.clone(),
            &format!(
                "/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id={selected_turn_id}&limit=1"
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{selected:?}");
        let items = selected["data"]["items"].as_array().unwrap();
        assert!(!items.is_empty());
        assert!(items.iter().all(|item| item["turn_id"] == selected_turn_id));
        assert_eq!(items[0]["content_preview"], expected_preview);
    }

    let events = EventIngestService::new(state.db())
        .list_events(session_id)
        .await
        .unwrap();
    let started_events: Vec<_> = events
        .iter()
        .filter(|event| event.event_type == EventType::TurnStarted)
        .collect();
    assert_eq!(started_events.len(), 7);
    assert!(started_events.iter().all(|event| {
        event.payload.get("topology_context").is_none()
            && event.payload.get("timeline_anchor").is_none()
    }));
    assert!(started_events.iter().all(|event| event.topology.is_some()));

    fs::remove_file(&transcript).unwrap();
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    assert_eq!(
        replay
            .turn("turn_pi_linear_3")
            .unwrap()
            .topology
            .parent_turn_id(),
        Some("turn_pi_linear_2")
    );
    assert_eq!(
        replay
            .turn("turn_pi_linear_5")
            .unwrap()
            .topology
            .parent_turn_id(),
        Some("turn_pi_linear_4")
    );
}
