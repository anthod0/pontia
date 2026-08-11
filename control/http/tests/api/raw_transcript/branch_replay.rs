use super::*;

#[tokio::test]
async fn branch_replay_resolves_root_middle_latest_and_abandoned_targets_without_mutation() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let state = test_state().await;
    let session_id = "sess_branch_resolve";
    let runtime_instance_id = "rtinst_branch_resolve";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;
    sqlx::query("UPDATE sessions SET state = 'idle' WHERE session_id = ?")
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();

    let session_key = "branch-resolve";
    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let segments = [
        concat!(
            "{\"type\":\"message\",\"id\":\"root-user\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"root\"}}\n",
            "{\"type\":\"message\",\"id\":\"root-answer\",\"parentId\":\"root-user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"root answer\"}]}}\n"
        ),
        concat!(
            "{\"type\":\"message\",\"id\":\"middle-user\",\"parentId\":\"root-answer\",\"message\":{\"role\":\"user\",\"content\":\"middle\"}}\n",
            "{\"type\":\"message\",\"id\":\"middle-answer\",\"parentId\":\"middle-user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"middle answer\"}]}}\n"
        ),
        concat!(
            "{\"type\":\"message\",\"id\":\"abandoned-user\",\"parentId\":\"middle-answer\",\"message\":{\"role\":\"user\",\"content\":\"abandoned\"}}\n",
            "{\"type\":\"message\",\"id\":\"abandoned-answer\",\"parentId\":\"abandoned-user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"abandoned answer\"}]}}\n"
        ),
        concat!(
            "{\"type\":\"message\",\"id\":\"latest-user\",\"parentId\":\"abandoned-answer\",\"message\":{\"role\":\"user\",\"content\":\"latest\"}}\n",
            "{\"type\":\"message\",\"id\":\"latest-answer\",\"parentId\":\"latest-user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"latest answer\"}]}}\n"
        ),
    ];
    let transcript = segments.concat();
    let transcript_path = session_dir.join(format!("2026-07-15T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(&transcript_path, &transcript).unwrap();
    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.display().to_string(),
            client_session_key: session_key.to_string(),
            client_session_file: Some(transcript_path.display().to_string()),
            metadata: json!({}),
        })
        .await
        .unwrap();
    let cursor = |offset, anchor: Option<&str>| {
        PiJsonlV2Cursor {
            binding_id: binding.id.clone(),
            byte_offset: offset,
            native_entry_anchor: anchor.map(ToString::to_string),
            relation: TimelineBoundaryRelation::After,
        }
        .encode()
    };
    let targets = [
        (
            "turn_01900000-0000-7000-8000-000000000001",
            "msg_branch_root",
            "root-user",
            "root-answer",
            "root",
            None,
            "completed",
            "root replacement",
        ),
        (
            "turn_01900000-0000-7000-8000-000000000002",
            "msg_branch_middle",
            "middle-user",
            "middle-answer",
            "linked",
            Some("turn_01900000-0000-7000-8000-000000000001"),
            "completed",
            "middle replacement",
        ),
        (
            "turn_01900000-0000-7000-8000-000000000003",
            "msg_branch_abandoned",
            "abandoned-user",
            "abandoned-answer",
            "linked",
            Some("turn_01900000-0000-7000-8000-000000000002"),
            "abandoned",
            "abandoned replacement",
        ),
        (
            "turn_01900000-0000-7000-8000-000000000004",
            "msg_branch_latest",
            "latest-user",
            "latest-answer",
            "linked",
            Some("turn_01900000-0000-7000-8000-000000000003"),
            "completed",
            "latest replacement",
        ),
    ];
    let head_anchors = [
        None,
        Some("root-answer"),
        Some("middle-answer"),
        Some("abandoned-answer"),
    ];
    let mut offset = 0;
    for (index, (turn_id, _, _, tail_anchor, topology, parent_turn_id, state_name, _)) in
        targets.iter().enumerate()
    {
        let tail = offset + segments[index].len();
        sqlx::query(
            r#"INSERT INTO turns
               (turn_id, session_id, head_cursor, tail_cursor, topology_status, parent_turn_id, state, input_summary)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(turn_id)
        .bind(session_id)
        .bind(cursor(offset, head_anchors[index]))
        .bind(cursor(tail, Some(tail_anchor)))
        .bind(topology)
        .bind(parent_turn_id)
        .bind(state_name)
        .bind(format!("{topology} input"))
        .execute(&state.db())
        .await
        .unwrap();
        offset = tail;
    }
    let capabilities = pontia_agent_clients::AgentClientCapabilities::pi_m0_default();
    sqlx::query(
        r#"INSERT INTO runtime_bindings
           (session_id, runtime_kind, runtime_instance_id, metadata)
           VALUES (?, 'pi_tui', ?, ?)"#,
    )
    .bind(session_id)
    .bind(runtime_instance_id)
    .bind(json!({"capabilities": capabilities}).to_string())
    .execute(&state.db())
    .await
    .unwrap();
    for (turn_id, message_id, _, _, _, _, _, replacement) in targets {
        sqlx::query(
            r#"INSERT INTO inbox_messages
               (message_id, session_id, state, delivery_policy, input_summary, branch_target_turn_id)
               VALUES (?, ?, 'dispatching', 'after_idle', ?, ?)"#,
        )
        .bind(message_id)
        .bind(session_id)
        .bind(replacement)
        .bind(turn_id)
        .execute(&state.db())
        .await
        .unwrap();
    }

    let before: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM turns), (SELECT COUNT(*) FROM events), (SELECT COUNT(*) FROM inbox_messages WHERE state = 'dispatching')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    for (_, message_id, target_entry_id, _, _, _, _, replacement_input) in targets {
        let (status, body) = post_internal_json(
            state.clone(),
            "/internal/v1/inbox/branch-replay/resolve",
            json!({
                "inbox_message_id": message_id,
                "session_id": session_id,
                "runtime_instance_id": runtime_instance_id,
                "client_type": "pi"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(
            body["data"]["branch_replay"],
            json!({
                "inbox_message_id": message_id,
                "session_id": session_id,
                "runtime_instance_id": runtime_instance_id,
                "client_type": "pi",
                "replacement_input": replacement_input,
                "target_entry_id": target_entry_id
            })
        );
    }
    let after: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM turns), (SELECT COUNT(*) FROM events), (SELECT COUNT(*) FROM inbox_messages WHERE state = 'dispatching')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(after, before);

    let inbox_uri = format!("/external/v1/sessions/{session_id}/inbox/messages");
    let (unknown_status, _) = post_external_json(
        state.clone(),
        &inbox_uri,
        None,
        json!({
            "input": "replacement",
            "branch_target_turn_id": "turn_unknown"
        }),
    )
    .await;
    assert_eq!(unknown_status, StatusCode::NOT_FOUND);

    let other_session_id = "sess_branch_other";
    seed_session(&state, other_session_id).await;
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, input_summary)
           VALUES ('turn_branch_other', ?, 'completed', 'other')"#,
    )
    .bind(other_session_id)
    .execute(&state.db())
    .await
    .unwrap();
    let (cross_session_status, _) = post_external_json(
        state.clone(),
        &inbox_uri,
        None,
        json!({
            "input": "replacement",
            "branch_target_turn_id": "turn_branch_other"
        }),
    )
    .await;
    assert_eq!(cross_session_status, StatusCode::CONFLICT);

    sqlx::query("UPDATE sessions SET state = 'busy' WHERE session_id = ?")
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();
    let (busy_status, _) = post_external_json(
        state.clone(),
        &inbox_uri,
        None,
        json!({
            "input": "replacement",
            "branch_target_turn_id": "turn_01900000-0000-7000-8000-000000000001"
        }),
    )
    .await;
    assert_eq!(busy_status, StatusCode::CONFLICT);
    sqlx::query("UPDATE sessions SET state = 'idle' WHERE session_id = ?")
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();

    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, input_summary)
           VALUES ('turn_branch_missing_boundary', ?, 'completed', 'missing boundary')"#,
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();
    let (missing_boundary_status, _) = post_external_json(
        state.clone(),
        &inbox_uri,
        None,
        json!({
            "input": "replacement",
            "branch_target_turn_id": "turn_branch_missing_boundary"
        }),
    )
    .await;
    assert_eq!(missing_boundary_status, StatusCode::CONFLICT);

    let unsupported_capabilities = pontia_agent_clients::AgentClientCapabilities::pi_m0_default();
    let mut unsupported_capabilities = unsupported_capabilities;
    unsupported_capabilities.branch_control = false;
    sqlx::query("UPDATE runtime_bindings SET metadata = ? WHERE session_id = ?")
        .bind(json!({"capabilities": unsupported_capabilities}).to_string())
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();
    let (non_writable_status, _) = post_external_json(
        state.clone(),
        &inbox_uri,
        None,
        json!({
            "input": "replacement",
            "branch_target_turn_id": "turn_01900000-0000-7000-8000-000000000001"
        }),
    )
    .await;
    assert_eq!(non_writable_status, StatusCode::UNPROCESSABLE_ENTITY);
    sqlx::query("UPDATE runtime_bindings SET metadata = ? WHERE session_id = ?")
        .bind(json!({"capabilities": capabilities}).to_string())
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();

    let (failed_delivery_status, failed_delivery_body) = post_external_json(
        state.clone(),
        &inbox_uri,
        None,
        json!({
            "input": "replacement with unavailable pane",
            "branch_target_turn_id": "turn_01900000-0000-7000-8000-000000000001"
        }),
    )
    .await;
    assert_eq!(failed_delivery_status, StatusCode::CREATED);
    assert_eq!(
        failed_delivery_body["data"]["inbox_message"]["state"],
        "failed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM turns WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .unwrap(),
        5
    );

    let (stale_status, _) = post_internal_json(
        state,
        "/internal/v1/inbox/branch-replay/resolve",
        json!({
            "inbox_message_id": "msg_branch_root",
            "session_id": session_id,
            "runtime_instance_id": "rtinst_stale",
            "client_type": "pi"
        }),
    )
    .await;
    assert_eq!(stale_status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn branch_replay_resolution_requires_internal_bearer_authentication() {
    let state = test_state().await;
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/inbox/branch-replay/resolve")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "inbox_message_id": "msg_unknown",
                        "session_id": "sess_unknown",
                        "runtime_instance_id": "rtinst_unknown",
                        "client_type": "pi"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn branch_inbox_delivery_is_opaque_idempotent_and_does_not_fabricate_a_turn() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let state = test_state().await;
    let session_id = "sess_branch_dispatch";
    let target_turn_id = "turn_branch_dispatch_target";
    let runtime_instance_id = "rtinst_branch_dispatch";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;
    sqlx::query("UPDATE sessions SET state = 'idle' WHERE session_id = ?")
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();

    let session_key = "branch-dispatch";
    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let transcript = concat!(
        "{\"type\":\"message\",\"id\":\"dispatch-user\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"original\"}}\n",
        "{\"type\":\"message\",\"id\":\"dispatch-answer\",\"parentId\":\"dispatch-user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer\"}]}}\n"
    );
    let transcript_path = session_dir.join(format!("2026-07-15T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(&transcript_path, transcript).unwrap();
    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.display().to_string(),
            client_session_key: session_key.to_string(),
            client_session_file: Some(transcript_path.display().to_string()),
            metadata: json!({}),
        })
        .await
        .unwrap();
    let cursor = |offset, anchor: Option<&str>| {
        PiJsonlV2Cursor {
            binding_id: binding.id.clone(),
            byte_offset: offset,
            native_entry_anchor: anchor.map(ToString::to_string),
            relation: TimelineBoundaryRelation::After,
        }
        .encode()
    };
    sqlx::query(
        r#"INSERT INTO turns
           (turn_id, session_id, head_cursor, tail_cursor, topology_status, state, input_summary)
           VALUES (?, ?, ?, ?, 'root', 'completed', 'original')"#,
    )
    .bind(target_turn_id)
    .bind(session_id)
    .bind(cursor(0, None))
    .bind(cursor(transcript.len(), Some("dispatch-answer")))
    .execute(&state.db())
    .await
    .unwrap();

    let tmux_session = format!("pontia_branch_dispatch_{}", std::process::id());
    let capture = temp.path().join("tmux-input.log");
    let command = format!("cat > {}", capture.display());
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", &tmux_session, &command])
        .stderr(Stdio::null())
        .status()
        .expect("spawn tmux");
    assert!(status.success());
    let tmux_value = |format: &str| {
        String::from_utf8(
            Command::new("tmux")
                .args(["display-message", "-p", "-t", &tmux_session, format])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    };
    let socket_path = tmux_value("#{socket_path}");
    let pane_id = tmux_value("#{pane_id}");
    let capabilities = pontia_agent_clients::AgentClientCapabilities::pi_m0_default();
    sqlx::query(
        r#"INSERT INTO runtime_bindings
           (session_id, runtime_kind, runtime_instance_id, tmux_socket_path, tmux_pane_id, metadata)
           VALUES (?, 'pi_tui', ?, ?, ?, ?)"#,
    )
    .bind(session_id)
    .bind(runtime_instance_id)
    .bind(&socket_path)
    .bind(&pane_id)
    .bind(
        json!({
            "runtime_instance_id": runtime_instance_id,
            "tmux_socket_path": socket_path,
            "tmux_pane_id": pane_id,
            "capabilities": capabilities
        })
        .to_string(),
    )
    .execute(&state.db())
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, source, client_type, event_type, occurred_at, payload)
           VALUES ('evt_branch_dispatch_ready', ?, 'agent_client', 'pi', 'session.ready',
                   '2026-07-24T00:00:00Z', ?)"#,
    )
    .bind(session_id)
    .bind(json!({"runtime_instance_id": runtime_instance_id}).to_string())
    .execute(&state.db())
    .await
    .unwrap();

    let uri = format!("/external/v1/sessions/{session_id}/inbox/messages");
    let request = json!({
        "input": "secret replacement content",
        "branch_target_turn_id": target_turn_id
    });
    let first = post_external_json(
        state.clone(),
        &uri,
        Some("branch-dispatch-once"),
        request.clone(),
    )
    .await;
    let second =
        post_external_json(state.clone(), &uri, Some("branch-dispatch-once"), request).await;
    assert_eq!(first.0, StatusCode::CREATED, "{:?}", first.1);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(first.1["data"], second.1["data"]);
    let message = &first.1["data"]["inbox_message"];
    let message_id = message["message_id"].as_str().unwrap();
    assert_eq!(message["state"], "dispatched");
    assert_eq!(message["branch_target_turn_id"], target_turn_id);
    assert_eq!(message["turn_id"], Value::Null);

    let expected = format!("/pontia-edit {message_id}\n");
    for _ in 0..20 {
        if fs::read_to_string(&capture).unwrap_or_default() == expected {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let captured = fs::read_to_string(&capture).unwrap_or_default();
    assert_eq!(captured, expected);
    assert!(!captured.contains("secret replacement content"));
    let turn_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(turn_count, 1);
    let current_turn_id: Option<String> =
        sqlx::query_scalar("SELECT current_turn_id FROM sessions WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(current_turn_id, None);
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", &tmux_session])
        .status();
}
