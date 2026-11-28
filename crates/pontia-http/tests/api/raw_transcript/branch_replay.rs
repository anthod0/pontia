use super::{
    AgentBindingService, AppState, Body, BodyExt, PiJsonlV2Cursor, Request, ServiceExt, StatusCode,
    TOKEN, TimelineBoundaryRelation, UpsertAgentBindingRequest, Value, fs, header, http, json,
    seed_session, tempdir, test_state,
};

use pontia_agent_control::ipc::rpc::{PROTOCOL_VERSION, RpcRequest};
use pontia_client_pi::ipc::serve_connection;
use pontia_client_pi::rpc::PiRpcPeer;
use std::sync::Arc;
use tokio::{net::UnixStream, sync::mpsc};

async fn rpc_client(state: &AppState) -> (Arc<PiRpcPeer>, mpsc::Receiver<RpcRequest>) {
    sqlx::query(
        "UPDATE session_runtimes SET process_fingerprint=? WHERE process_fingerprint IS NULL",
    )
    .bind(
        serde_json::to_string(
            &pontia_runtime::ProcessFingerprint::capture(std::process::id()).unwrap(),
        )
        .unwrap(),
    )
    .execute(&state.db())
    .await
    .unwrap();
    let (server, client) = UnixStream::pair().unwrap();
    tokio::spawn(serve_connection(state.control().client_context(), server));
    PiRpcPeer::new(client)
}

async fn attach(pi: &PiRpcPeer, session: &str, runtime: &str, native: &str) {
    pi.call(
        "runtime.attach",
        json!({
            "version": PROTOCOL_VERSION, "session_id": session,
            "runtime_id": runtime, "client_session_key": native,
        }),
    )
    .await
    .unwrap();
}

async fn post_external_json(state: AppState, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, body)
}

async fn seed_replay_target(
    state: &AppState,
    root: &std::path::Path,
    session_id: &str,
    target_turn_id: &str,
    runtime_id: &str,
    session_key: &str,
) {
    seed_session(state, session_id).await;
    sqlx::query("UPDATE sessions SET state = 'idle' WHERE session_id = ?")
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();
    let transcript = concat!(
        "{\"type\":\"message\",\"id\":\"target-user\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"original\"}}\n",
        "{\"type\":\"message\",\"id\":\"target-answer\",\"parentId\":\"target-user\",\"message\":{\"role\":\"assistant\",\"content\":[]}}\n"
    );
    let transcript_path = root.join("session.jsonl");
    fs::write(&transcript_path, transcript).unwrap();
    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: root.display().to_string(),
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
    .bind(cursor(transcript.len(), Some("target-answer")))
    .execute(&state.db())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE session_runtimes SET runtime_id=?,tmux_socket_path='/unused/branch.sock',tmux_pane_id='%1' WHERE session_id=?",
    )
    .bind(runtime_id)
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, source, client_type, event_type, occurred_at, payload)
           VALUES (?, ?, 'agent_client', 'pi', 'session.ready',
                   '2026-11-27T00:00:00Z', ?)"#,
    )
    .bind(format!("evt_{session_id}_ready"))
    .bind(session_id)
    .bind(json!({"runtime_id": runtime_id}).to_string())
    .execute(&state.db())
    .await
    .unwrap();
}

#[tokio::test]
async fn branch_replay_uses_one_time_memory_without_creating_an_inbox_message() {
    let temp = tempdir().unwrap();
    let (state, _pontia_home) = test_state().await;
    let session_id = "sess_branch_replay";
    let target_turn_id = "turn_branch_target";
    let runtime_id = "runtime_branch_replay";
    let session_key = "branch-native";
    seed_replay_target(
        &state,
        temp.path(),
        session_id,
        target_turn_id,
        runtime_id,
        session_key,
    )
    .await;

    let (pi, mut requests) = rpc_client(&state).await;
    attach(&pi, session_id, runtime_id, session_key).await;
    let client = pi.clone();
    let replay = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        assert_eq!(request.method, "branch.replay");
        let replay_id = request.params["replay_id"].as_str().unwrap().to_string();
        assert!(replay_id.starts_with("replay_"));
        client
            .reply(request.id, json!({"accepted": true}))
            .await
            .unwrap();
        let params = json!({
            "replay_id": replay_id,
            "session_id": session_id,
            "runtime_id": runtime_id,
            "client_type": "pi",
        });
        let resolved = client.call("branch.resolve", params.clone()).await.unwrap();
        assert_eq!(
            resolved["branch_replay"]["replacement_input"],
            "replacement"
        );
        assert_eq!(resolved["branch_replay"]["target_entry_id"], "target-user");
        assert!(client.call("branch.resolve", params).await.is_err());
        replay_id
    });

    let (status, body) = post_external_json(
        state.clone(),
        &format!("/api/v1/sessions/{session_id}/branch-replay"),
        json!({"target_turn_id": target_turn_id, "input": "replacement"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(
        body["data"]["branch_replay"]["replay_id"],
        replay.await.unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM inbox_messages WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM turns WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .unwrap(),
        1
    );
    pi.close();
}

#[tokio::test]
async fn branch_replay_validates_idle_and_target_before_delivery() {
    let temp = tempdir().unwrap();
    let (state, _pontia_home) = test_state().await;
    let session_id = "sess_branch_validation";
    let target_turn_id = "turn_branch_validation";
    let runtime_id = "runtime_branch_validation";
    seed_replay_target(
        &state,
        temp.path(),
        session_id,
        target_turn_id,
        runtime_id,
        "validation-native",
    )
    .await;
    let uri = format!("/api/v1/sessions/{session_id}/branch-replay");

    let (status, _) = post_external_json(
        state.clone(),
        &uri,
        json!({"target_turn_id": "turn_missing", "input": "replacement"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    sqlx::query("UPDATE sessions SET state = 'busy' WHERE session_id = ?")
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();
    let (status, _) = post_external_json(
        state.clone(),
        &uri,
        json!({"target_turn_id": target_turn_id, "input": "replacement"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _) = post_external_json(
        state.clone(),
        &format!("/api/v1/sessions/{session_id}/messages"),
        json!({"input": "replacement", "branch_target_turn_id": target_turn_id}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn branch_resolution_requires_registered_connection_identity() {
    let (state, _pontia_home) = test_state().await;
    let (pi, _requests) = rpc_client(&state).await;
    let error = pi
        .call(
            "branch.resolve",
            json!({
                "replay_id": "replay_unknown", "session_id": "sess_unknown",
                "runtime_id": "runtime_unknown", "client_type": "pi",
            }),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("requires registration"));
    pi.close();
}
