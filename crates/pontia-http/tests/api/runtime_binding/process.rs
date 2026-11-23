use super::{
    StatusCode, get_session_context_by_client_session, json, post_upsert, test_state, upsert_body,
};

#[tokio::test]
async fn registration_tracks_the_ipc_peer_without_a_terminal_binding() {
    let (state, _app) = test_state().await;
    let workspace = crate::common::test_app::registered_workspace(&state).await;
    let (status, registered) = post_upsert(
        state.clone(),
        upsert_body(workspace.path().to_str().unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{registered}");
    let runtime = registered["runtime"]["runtime_id"].as_str().unwrap();
    let (fingerprint, socket, pane): (String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT process_fingerprint, tmux_socket_path, tmux_pane_id FROM session_runtimes WHERE runtime_id=?"
    ).bind(runtime).fetch_one(&state.db()).await.unwrap();
    let fingerprint: pontia_runtime::ProcessFingerprint =
        serde_json::from_str(&fingerprint).unwrap();
    assert_eq!(fingerprint.pid, std::process::id());
    assert_eq!(
        fingerprint.observe(),
        pontia_runtime::ProcessObservation::Alive
    );
    assert_eq!((socket, pane), (None, None));
}

#[tokio::test]
async fn pending_native_identity_recovers_the_launch_and_preserves_its_terminal_location() {
    let (state, _app) = test_state().await;
    let workspace = crate::common::test_app::registered_workspace(&state).await;
    sqlx::query("INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_pending', 'pi', 'starting')")
        .execute(&state.db()).await.unwrap();
    sqlx::query("INSERT INTO session_runtimes (runtime_id, session_id, role, state, tmux_socket_path, tmux_pane_id, start_command, created_at) VALUES ('rt_pending', 'sess_pending', 'pi_tui', 'starting', '/launch/socket', '%9', 'pi --approve', '2026-01-01T00:00:00Z')")
        .execute(&state.db()).await.unwrap();
    let (status, context) =
        get_session_context_by_client_session(state.clone(), "pi", "sess_pending").await;
    assert_eq!(status, StatusCode::OK, "{context}");
    assert_eq!(
        context["data"]["session_context"]["runtime_id"],
        "rt_pending"
    );
    let mut request = upsert_body(workspace.path().to_str().unwrap());
    request["client_session_key"] = json!("sess_pending");
    request["runtime_id"] = json!("rt_pending");
    request.as_object_mut().unwrap().remove("start_command");
    let (status, registered) = post_upsert(state.clone(), request).await;
    assert_eq!(status, StatusCode::OK, "{registered}");
    assert_eq!(registered["session"]["session_id"], "sess_pending");
    let location: (String, String, String) = sqlx::query_as("SELECT tmux_socket_path, tmux_pane_id, start_command FROM session_runtimes WHERE runtime_id='rt_pending'")
        .fetch_one(&state.db()).await.unwrap();
    assert_eq!(
        location,
        ("/launch/socket".into(), "%9".into(), "pi --approve".into())
    );
}
