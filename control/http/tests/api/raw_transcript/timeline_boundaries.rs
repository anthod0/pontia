use super::{
    AgentBindingService, CapturedLogWriter, EventIngestService, EventSource, EventType,
    ProjectionState, ReportedEvent, StatusCode, TimelineBoundary, UpsertAgentBindingRequest, Value,
    WithSubscriber, Write, fs, get_json, json, post_internal_event, post_pi_turn_event,
    precreate_turn_if_missing, seed_session, tempdir, test_state,
};

#[tokio::test]
async fn first_turn_timeline_survives_pi_creating_its_jsonl_after_turn_start() {
    let temp = tempdir().unwrap();
    let state = test_state().await;
    let session_id = "sess_delayed_first_timeline";
    let session_key = "delayed-first-timeline";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let transcript = temp.path().join(format!("bound-{session_key}.jsonl"));
    let binding = AgentBindingService::new(state.db())
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

    precreate_turn_if_missing(&state, session_id, "turn_delayed_first").await;
    let (status, body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": "turn_delayed_first",
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_projected_timeline",
                "timeline_anchor": { "previous_leaf_id": "previous" },
                "topology_context": { "entries": [
                    {"id": "previous", "kind": "model_change"}
                ] }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let started_turn = EventIngestService::new(state.db())
        .get_turn("turn_delayed_first")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        started_turn.head_cursor.as_deref(),
        Some(format!("pi-jsonl-v2:{}:0:after:previous", binding.id).as_str())
    );
    assert_eq!(
        started_turn.topology,
        pontia_core::domain::TurnTopology::Root
    );

    let (pending_status, pending_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(pending_status, StatusCode::OK, "{pending_body:?}");
    assert_eq!(pending_body["data"]["items"], json!([]));
    assert!(pending_body["data"]["next_turn_id"].is_null());

    fs::write(
        &transcript,
        concat!(
            "{\"type\":\"session\",\"id\":\"native-session\"}\n",
            "{\"type\":\"model_change\",\"id\":\"previous\",\"parentId\":null}\n",
            "{\"type\":\"message\",\"id\":\"user\",\"parentId\":\"previous\",\"message\":{\"role\":\"user\",\"content\":\"first question\"}}\n",
            "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"first answer\"}]}}\n",
        ),
    )
    .unwrap();
    let (active_status, active_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(active_status, StatusCode::OK, "{active_body:?}");
    assert_eq!(active_body["data"]["items"].as_array().unwrap().len(), 2);
    let discovered: bool = sqlx::query_scalar("SELECT discovered FROM agent_bindings WHERE id = ?")
        .bind(&binding.id)
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert!(discovered);

    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_delayed_first",
        "evt_delayed_first_completed",
        "turn.completed",
        json!({ "terminal_leaf_id": "answer" }),
    )
    .await;

    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(
        body["data"]["items"][0]["content_preview"],
        "first question"
    );
    assert_eq!(body["data"]["items"][1]["content_preview"], "first answer");
}

#[tokio::test]
async fn delayed_terminal_fact_seals_timeline_after_runtime_binding_changes() {
    let temp = tempdir().unwrap();
    let state = test_state().await;
    let session_id = "sess_delayed_terminal";
    let session_key = "delayed-terminal";
    let turn_id = "turn_delayed_terminal";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let transcript = temp.path().join(format!("bound-{session_key}.jsonl"));
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
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, metadata) VALUES (?, 'pi_tui', 'rtinst_a', '{}')",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();

    fs::write(
        &transcript,
        b"{\"type\":\"model_change\",\"id\":\"previous\",\"parentId\":null}\n",
    )
    .unwrap();

    precreate_turn_if_missing(&state, session_id, turn_id).await;
    let (started_status, started_body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_a",
                "previous_leaf_id": "previous",
                "topology_context": { "entries": [
                    {"id": "previous", "kind": "model_change"}
                ] }
            }
        }),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK, "{started_body:?}");

    fs::write(
        &transcript,
        concat!(
            "{\"type\":\"model_change\",\"id\":\"previous\",\"parentId\":null}\n",
            "{\"type\":\"message\",\"id\":\"user\",\"parentId\":\"previous\",\"message\":{\"role\":\"user\",\"content\":\"question\"}}\n",
            "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer\"}]}}\n",
        ),
    )
    .unwrap();
    sqlx::query(
        "UPDATE runtime_bindings SET runtime_instance_id = 'rtinst_b' WHERE session_id = ?",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();

    let (output_status, output_body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.output",
            "data": { "output_summary": "answer" }
        }),
    )
    .await;
    assert_eq!(output_status, StatusCode::OK, "{output_body:?}");

    let (completed_status, completed_body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.completed",
            "data": {
                "runtime_instance_id": "rtinst_a",
                "terminal_leaf_id": "answer"
            }
        }),
    )
    .await;
    assert_eq!(completed_status, StatusCode::OK, "{completed_body:?}");

    let (timeline_status, timeline_body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(timeline_status, StatusCode::OK, "{timeline_body:?}");
    assert_eq!(timeline_body["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        timeline_body["data"]["items"][0]["content_preview"],
        "question"
    );
    assert_eq!(
        timeline_body["data"]["items"][1]["content_preview"],
        "answer"
    );
}

#[tokio::test]
async fn hook_lifecycle_events_capture_project_and_replay_pi_v2_boundaries() {
    let temp = tempdir().unwrap();

    let state = test_state().await;
    let session_id = "sess_pi_boundaries";
    let turn_id = "turn_pi_boundaries";
    let session_key = "pi-boundary-session";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let transcript = temp.path().join(format!("bound-{session_key}.jsonl"));
    fs::write(
        &transcript,
        b"{\"type\":\"message\",\"id\":\"previous_leaf\",\"parentId\":null}\n",
    )
    .unwrap();
    let head_offset = fs::metadata(&transcript).unwrap().len();

    let binding = AgentBindingService::new(state.db())
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

    precreate_turn_if_missing(&state, session_id, turn_id).await;
    let started = json!({
        "session_id": session_id,
        "turn_id": turn_id,
        "type": "turn.started",
        "data": {
            "runtime_instance_id": "rtinst_pi_boundary",
            "timeline_anchor": { "previous_leaf_id": "previous_leaf" }
        }
    });
    assert_eq!(
        post_internal_event(state.clone(), started).await.0,
        StatusCode::OK
    );

    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"user_leaf\",\"parentId\":\"previous_leaf\"}\n",
                "{\"type\":\"message\",\"id\":\"terminal_leaf\",\"parentId\":\"user_leaf\"}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let tail_offset = fs::metadata(&transcript).unwrap().len();

    let completed = json!({
        "session_id": session_id,
        "turn_id": turn_id,
        "type": "turn.completed",
        "data": {
            "runtime_instance_id": "rtinst_pi_boundary",
            "timeline_anchor": { "terminal_leaf_id": "terminal_leaf" }
        }
    });
    assert_eq!(
        post_internal_event(state.clone(), completed).await.0,
        StatusCode::OK
    );

    let expected_head = format!(
        "pi-jsonl-v2:{}:{head_offset}:after:previous_leaf",
        binding.id
    );
    let expected_tail = format!(
        "pi-jsonl-v2:{}:{tail_offset}:after:terminal_leaf",
        binding.id
    );
    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert!(body["data"]["turn"].get("turn_index").is_none());
    assert!(body["data"]["turn"].get("head_cursor").is_none());
    assert!(body["data"]["turn"].get("tail_cursor").is_none());

    let events = EventIngestService::new(state.db())
        .list_events(session_id)
        .await
        .unwrap();
    let started_event = events
        .iter()
        .find(|event| event.event_type == EventType::TurnStarted)
        .expect("started event");
    assert!(started_event.payload.get("timeline_anchor").is_none());
    assert_eq!(
        started_event.timeline_boundary,
        Some(TimelineBoundary::head(expected_head.clone()))
    );
    assert!(started_event.payload.get("timeline_boundary").is_none());
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    let replayed = replay.turn(turn_id).unwrap();
    assert_eq!(
        replayed.head_cursor.as_deref(),
        Some(expected_head.as_str())
    );
    assert_eq!(
        replayed.tail_cursor.as_deref(),
        Some(expected_tail.as_str())
    );
}

#[tokio::test]
async fn interrupted_pi_turn_captures_tail_boundary_and_remains_timeline_readable() {
    let temp = tempdir().unwrap();

    let state = test_state().await;
    let session_id = "sess_pi_interrupted_boundary";
    let turn_id = "turn_pi_interrupted_boundary";
    let session_key = "pi-interrupted-boundary";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let transcript = temp.path().join(format!("bound-{session_key}.jsonl"));
    fs::write(
        &transcript,
        b"{\"type\":\"message\",\"id\":\"previous_leaf\",\"parentId\":null}\n",
    )
    .unwrap();
    let head_offset = fs::metadata(&transcript).unwrap().len();

    let binding = AgentBindingService::new(state.db())
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

    precreate_turn_if_missing(&state, session_id, turn_id).await;
    let (status, body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_pi_interrupted_boundary",
                "previous_leaf_id": "previous_leaf"
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"user_leaf\",\"parentId\":\"previous_leaf\",\"message\":{\"role\":\"user\",\"content\":\"question\"}}\n",
                "{\"type\":\"message\",\"id\":\"terminal_leaf\",\"parentId\":\"user_leaf\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"partial answer\"}],\"stopReason\":\"aborted\"}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let tail_offset = fs::metadata(&transcript).unwrap().len();

    let (status, body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.interrupted",
            "data": {
                "runtime_instance_id": "rtinst_pi_interrupted_boundary",
                "terminal_leaf_id": "terminal_leaf"
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 2);

    let expected_head = format!(
        "pi-jsonl-v2:{}:{head_offset}:after:previous_leaf",
        binding.id
    );
    let expected_tail = format!(
        "pi-jsonl-v2:{}:{tail_offset}:after:terminal_leaf",
        binding.id
    );
    let turn = EventIngestService::new(state.db())
        .get_turn(turn_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(turn.head_cursor.as_deref(), Some(expected_head.as_str()));
    assert_eq!(turn.tail_cursor.as_deref(), Some(expected_tail.as_str()));

    let events = EventIngestService::new(state.db())
        .list_events(session_id)
        .await
        .unwrap();
    let interrupted = events
        .iter()
        .find(|event| event.event_type == EventType::TurnInterrupted)
        .expect("interrupted event");
    assert_eq!(
        interrupted.timeline_boundary,
        Some(TimelineBoundary::tail(expected_tail))
    );
    assert!(interrupted.payload.get("timeline_anchor").is_none());
}

#[tokio::test]
async fn timeline_capture_failure_keeps_lifecycle_fact_and_logs_structured_warning() {
    let temp = tempdir().unwrap();
    let state = test_state().await;
    let session_id = "sess_pi_boundary_missing";
    seed_session(&state, session_id).await;
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            "evt_existing_created".to_string(),
            session_id.to_string(),
            Some("turn_existing".to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            "evt_existing_completed".to_string(),
            session_id.to_string(),
            Some("turn_existing".to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCompleted,
            json!({}),
        ))
        .await
        .unwrap();
    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: temp.path().join("workspace").display().to_string(),
            client_session_key: "missing-session".to_string(),
            client_session_file: Some(
                temp.path()
                    .join("missing-session.jsonl")
                    .display()
                    .to_string(),
            ),
            metadata: json!({}),
        })
        .await
        .unwrap();
    precreate_turn_if_missing(&state, session_id, "turn_pi_boundary_missing").await;
    let started = json!({
        "session_id": session_id,
        "turn_id": "turn_pi_boundary_missing",
        "type": "turn.started",
        "data": {
            "runtime_instance_id": "rtinst_pi_boundary_missing",
            "timeline_anchor": { "previous_leaf_id": null }
        }
    });

    let captured_logs = CapturedLogWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_writer(captured_logs.clone())
        .finish();
    let (status, body) = post_internal_event(state.clone(), started)
        .with_subscriber(subscriber)
        .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let turn = EventIngestService::new(state.db())
        .get_turn("turn_pi_boundary_missing")
        .await
        .unwrap()
        .unwrap();
    assert!(turn.head_cursor.is_none());
    let warning = captured_logs
        .text()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|entry| entry["fields"]["code"] == "timeline_boundary_capture_failed")
        .expect("structured timeline capture warning");
    assert_eq!(warning["level"], "WARN");
    assert!(
        warning["fields"]["event_id"]
            .as_str()
            .is_some_and(|event_id| event_id.starts_with("evt_"))
    );
    assert_eq!(warning["fields"]["session_id"], session_id);
    assert_eq!(warning["fields"]["turn_id"], "turn_pi_boundary_missing");
    assert_eq!(warning["fields"]["event_type"], "turn.started");
    assert_eq!(warning["fields"]["client_type"], "pi");
    assert_eq!(warning["fields"]["binding_id"], binding.id);
    assert_eq!(warning["fields"]["adapter_error"], "source_unavailable");
    assert!(
        !captured_logs
            .text()
            .contains(&temp.path().display().to_string())
    );
}
