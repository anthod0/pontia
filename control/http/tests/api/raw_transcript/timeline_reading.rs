use super::{
    AgentBindingService, AppState, EventIngestService, EventSource, EventType, PathBuf,
    ReportedEvent, StatusCode, UpsertAgentBindingRequest, Value, Write, fs, get_json, json,
    post_internal_event, post_pi_turn_event, precreate_turn_if_missing, seed_session,
    seed_session_for_client, tempdir, test_state,
};

struct ActivePiTimelineFixture {
    _temp: tempfile::TempDir,
    state: AppState,
    session_id: &'static str,
    transcript: PathBuf,
}

async fn active_pi_timeline_fixture(
    session_id: &'static str,
    session_key: &str,
    turn_id: &str,
    _started_event_id: &str,
) -> ActivePiTimelineFixture {
    let temp = tempdir().unwrap();
    let state = test_state().await;
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let transcript = temp.path().join(format!("bound-{session_key}.jsonl"));
    fs::write(
        &transcript,
        b"{\"type\":\"message\",\"id\":\"root\",\"parentId\":null}\n",
    )
    .unwrap();
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
    precreate_turn_if_missing(&state, session_id, turn_id).await;
    let (status, body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_projected_timeline",
                "timeline_anchor": { "previous_leaf_id": "root" },
                "topology_context": { "entries": [] },
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    ActivePiTimelineFixture {
        _temp: temp,
        state,
        session_id,
        transcript,
    }
}

#[tokio::test]
async fn claude_session_uses_the_linear_turn_timeline_endpoint() {
    let temp = tempdir().unwrap();
    let state = test_state().await;
    let session_id = "sess_claude_linear_timeline";
    let turn_id = "turn_claude_linear_timeline";
    seed_session_for_client(&state, session_id, "claude").await;

    let transcript = temp.path().join("claude-session.jsonl");
    let metadata = b"{\"type\":\"system\",\"subtype\":\"turn_duration\"}\n";
    let prompt = b"{\"type\":\"user\",\"uuid\":\"claude-user\",\"timestamp\":\"2026-07-15T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"Claude question\"}}\n";
    fs::write(
        &transcript,
        [metadata.as_slice(), prompt.as_slice()].concat(),
    )
    .unwrap();
    AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "claude".to_string(),
            launch_cwd: temp.path().display().to_string(),
            client_session_key: "claude-native-session".to_string(),
            client_session_file: Some(transcript.display().to_string()),
            metadata: json!({}),
        })
        .await
        .unwrap();
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            "evt_claude_turn_created".to_string(),
            session_id.to_string(),
            Some(turn_id.to_string()),
            EventSource::ExternalApi,
            "claude".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();

    let (status, started) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_claude_timeline" }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started:?}");

    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"assistant\",\"uuid\":\"claude-thinking\",\"timestamp\":\"2026-07-15T00:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"inspect first\"}]}}\n",
                "{\"type\":\"assistant\",\"uuid\":\"claude-answer\",\"timestamp\":\"2026-07-15T00:00:03Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Claude answer\"}]}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let (status, completed) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.completed",
            "data": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed:?}");

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["direction"], "backward");
    assert_eq!(body["data"]["next_turn_id"], Value::Null);
    assert_eq!(
        body["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["user", "thinking", "assistant"]
    );
    assert_eq!(
        body["data"]["items"][0]["content_preview"],
        "Claude question"
    );
    assert_eq!(body["data"]["items"][2]["content_preview"], "Claude answer");
}

#[tokio::test]
async fn claude_timeline_includes_assistant_appended_after_turn_completion() {
    let temp = tempdir().unwrap();
    let state = test_state().await;
    let session_id = "sess_claude_delayed_assistant";
    let turn_id = "turn_claude_delayed_assistant";
    seed_session_for_client(&state, session_id, "claude").await;

    let transcript = temp.path().join("claude-session.jsonl");
    fs::write(
        &transcript,
        concat!(
            "{\"type\":\"system\",\"subtype\":\"turn_duration\"}\n",
            "{\"type\":\"user\",\"uuid\":\"claude-user\",\"timestamp\":\"2026-07-15T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"Claude question\"}}\n"
        ),
    )
    .unwrap();
    AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "claude".to_string(),
            launch_cwd: temp.path().display().to_string(),
            client_session_key: "claude-delayed-assistant".to_string(),
            client_session_file: Some(transcript.display().to_string()),
            metadata: json!({}),
        })
        .await
        .unwrap();
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            "evt_claude_delayed_created".to_string(),
            session_id.to_string(),
            Some(turn_id.to_string()),
            EventSource::ExternalApi,
            "claude".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();

    let (status, started) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_claude_delayed" }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started:?}");

    let (status, completed) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.completed",
            "data": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed:?}");

    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"assistant\",\"uuid\":\"claude-answer\",\"timestamp\":\"2026-07-15T00:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Claude answer\"}]}}\n",
                "{\"type\":\"user\",\"uuid\":\"next-claude-user\",\"timestamp\":\"2026-07-15T00:01:00Z\",\"message\":{\"role\":\"user\",\"content\":\"Next question\"}}\n"
            )
            .as_bytes(),
        )
        .unwrap();

    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(
        body["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["user", "assistant"]
    );
    assert_eq!(body["data"]["items"][1]["content_preview"], "Claude answer");
}

#[tokio::test]
async fn turn_timeline_reads_sealed_pi_ranges_and_pages_by_turn_id() {
    let temp = tempdir().unwrap();
    let state = test_state().await;
    let session_id = "sess_projected_timeline";
    let session_key = "projected-timeline";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let transcript = temp.path().join(format!("bound-{session_key}.jsonl"));
    fs::write(
        &transcript,
        b"{\"type\":\"message\",\"id\":\"root\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"before\"}}\n",
    )
    .unwrap();
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

    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_one",
        "evt_turn_one_started",
        "turn.started",
        json!({ "previous_leaf_id": "root" }),
    )
    .await;
    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"u1\",\"parentId\":\"root\",\"timestamp\":\"2026-07-15T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"question one\"}}\n",
                "{\"type\":\"message\",\"id\":\"a1\",\"parentId\":\"u1\",\"timestamp\":\"2026-07-15T00:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer one\"},{\"type\":\"toolCall\",\"name\":\"read\",\"arguments\":{\"path\":\"README.md\"}}]}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_one",
        "evt_turn_one_completed",
        "turn.completed",
        json!({ "terminal_leaf_id": "a1" }),
    )
    .await;

    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_two",
        "evt_turn_two_started",
        "turn.started",
        json!({ "previous_leaf_id": "a1" }),
    )
    .await;
    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"u2\",\"parentId\":\"a1\",\"timestamp\":\"2026-07-15T00:00:03Z\",\"message\":{\"role\":\"user\",\"content\":\"question two\"}}\n",
                "{\"type\":\"message\",\"id\":\"a2\",\"parentId\":\"u2\",\"timestamp\":\"2026-07-15T00:00:04Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer two\"}]}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_two",
        "evt_turn_two_completed",
        "turn.completed",
        json!({ "terminal_leaf_id": "a2" }),
    )
    .await;

    let (status, recent) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recent:?}");
    assert_eq!(recent["data"]["next_turn_id"], "turn_one");
    assert_eq!(recent["data"]["items"].as_array().unwrap().len(), 2);
    assert!(
        recent["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["turn_id"] == "turn_two")
    );

    let (status, older) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/timeline?direction=backward&turn_id=turn_one&limit=1"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{older:?}");
    assert!(older["data"]["next_turn_id"].is_null());
    assert_eq!(older["data"]["items"][0]["content_preview"], "question one");
    assert_eq!(
        older["data"]["items"][2]["managed_tool_use"]["tool_name"],
        "read"
    );

    let (status, all) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{all:?}");
    let turn_ids = all["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["turn_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        turn_ids,
        vec!["turn_one", "turn_one", "turn_one", "turn_two", "turn_two"]
    );
}

#[tokio::test]
async fn turn_timeline_reads_growing_active_output_without_persisting_temporary_boundaries() {
    let fixture = active_pi_timeline_fixture(
        "sess_active_turn_empty",
        "active-turn-empty",
        "turn_active",
        "evt_active_turn_started",
    )
    .await;
    let state = fixture.state.clone();
    let session_id = fixture.session_id;
    let transcript = &fixture.transcript;

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["items"], json!([]));
    assert!(body["data"]["next_turn_id"].is_null());

    fs::OpenOptions::new()
        .append(true)
        .open(transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"user\",\"parentId\":\"root\",\"message\":{\"role\":\"user\",\"content\":\"question\"}}\n",
                "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"partial answer\"},{\"type\":\"toolCall\",\"id\":\"call_read\",\"name\":\"read\",\"arguments\":{\"path\":\"README.md\"}}]}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let (status, growing) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id=turn_active&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{growing:?}");
    assert_eq!(growing["data"]["items"].as_array().unwrap().len(), 3);
    assert_eq!(
        growing["data"]["items"][1]["content_preview"],
        "partial answer"
    );
    assert_eq!(growing["data"]["items"][2]["kind"], "tool_call");
    assert_eq!(
        growing["data"]["items"][2]["managed_tool_use"]["tool_name"],
        "read"
    );
    let (status, growing_tree) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_active"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{growing_tree:?}");
    assert_eq!(growing_tree["data"]["groups"][0]["turn_id"], "turn_active");
    assert_eq!(
        growing_tree["data"]["groups"][0]["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let active_turn = EventIngestService::new(state.db())
        .get_turn("turn_active")
        .await
        .unwrap()
        .unwrap();
    assert!(active_turn.head_cursor.is_some());
    assert!(active_turn.tail_cursor.is_none());

    fs::OpenOptions::new()
        .append(true)
        .open(transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"tool_result\",\"parentId\":\"answer\",\"message\":{\"role\":\"toolResult\",\"toolCallId\":\"call_read\",\"toolName\":\"read\",\"content\":[{\"type\":\"text\",\"text\":\"README contents\"}],\"isError\":false}}\n",
                "{\"type\":\"message\",\"id\":\"final\",\"parentId\":\"tool_result\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"final answer\"}]}}\n"
            ).as_bytes(),
        )
        .unwrap();
    let (status, grown) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id=turn_active&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grown:?}");
    assert_eq!(grown["data"]["items"].as_array().unwrap().len(), 5);
    assert_eq!(grown["data"]["items"][3]["kind"], "tool_result");
    assert_eq!(grown["data"]["items"][4]["content_preview"], "final answer");
    let (status, grown_tree) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_active"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grown_tree:?}");
    assert_eq!(
        grown_tree["data"]["groups"][0]["items"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        grown_tree["data"]["groups"][0]["items"][4]["content_preview"],
        "final answer"
    );
    assert!(
        EventIngestService::new(state.db())
            .get_turn("turn_active")
            .await
            .unwrap()
            .unwrap()
            .tail_cursor
            .is_none()
    );

    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_active",
        "evt_active_turn_completed",
        "turn.completed",
        json!({ "terminal_leaf_id": "final" }),
    )
    .await;
    let (status, sealed) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sealed:?}");
    assert_eq!(sealed["data"]["items"], grown["data"]["items"]);
    assert!(
        EventIngestService::new(state.db())
            .get_turn("turn_active")
            .await
            .unwrap()
            .unwrap()
            .tail_cursor
            .is_some()
    );
}

#[tokio::test]
async fn turn_timeline_rejects_unassignable_active_pi_entries() {
    let fixture = active_pi_timeline_fixture(
        "sess_active_turn_unassignable",
        "active-turn-unassignable",
        "turn_active_invalid",
        "evt_active_turn_invalid_started",
    )
    .await;
    let state = fixture.state.clone();
    let session_id = fixture.session_id;
    let transcript = &fixture.transcript;
    fs::OpenOptions::new()
        .append(true)
        .open(transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"parentId\":\"root\",\"message\":{\"role\":\"user\",\"content\":\"missing id\"}}\n",
                "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"root\",\"message\":{\"role\":\"assistant\",\"content\":\"answer\"}}\n"
            )
            .as_bytes(),
        )
        .unwrap();

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_invalid");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("turn_active_invalid")
    );
}
