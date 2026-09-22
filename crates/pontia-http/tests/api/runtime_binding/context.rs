use super::{
    StatusCode, get_current_turn_by_client_session, get_session_context_by_client_session,
    test_state,
};
#[tokio::test]
async fn internal_agent_binding_session_context_returns_stable_runtime_without_an_active_turn() {
    let (state, _app) = test_state().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES ('sess_context', 'pi', 'idle', NULL, '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, internal_event_url)
           VALUES ('sess_context', 'pi_tui', 'rtinst_stable', 'http://127.0.0.1:18080/internal/v1/events')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert runtime binding");
    sqlx::query(
        r#"INSERT INTO agent_bindings (id, session_id, client_type, launch_cwd, client_session_key, metadata)
           VALUES ('binding_context', 'sess_context', 'pi', '/repo', 'pi_context', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert agent binding");

    let (status, response) = get_session_context_by_client_session(state, "pi", "pi_context").await;

    assert_eq!(status, StatusCode::OK, "{response:?}");
    let context = &response["data"]["session_context"];
    assert_eq!(context["session_id"], "sess_context");
    assert_eq!(context["session_state"], "idle");
    assert_eq!(context["client_type"], "pi");
    assert_eq!(context["client_session_key"], "pi_context");
    assert_eq!(context["runtime_instance_id"], "rtinst_stable");
    assert_eq!(
        context["internal_event_url"],
        "http://127.0.0.1:18080/internal/v1/events"
    );
}

#[tokio::test]
async fn internal_agent_binding_current_turn_returns_active_turn_context_by_client_session() {
    let (state, _app) = test_state().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES ('sess_current', 'pi', 'busy', 'turn_current', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, input_summary, metadata)
           VALUES ('turn_current', 'sess_current', 'running', 'work', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert turn");
    sqlx::query(
        r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, launch_cwd, internal_event_url)
           VALUES ('sess_current', 'pi_tui', 'rtinst_current', '/repo', 'http://127.0.0.1:18080/internal/v1/events')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert runtime binding");
    sqlx::query(
        r#"INSERT INTO agent_bindings (id, session_id, client_type, launch_cwd, client_session_key, metadata)
           VALUES ('binding_current', 'sess_current', 'pi', '/repo', 'pi_session_123', '{"transcript_path":"/tmp/pi.jsonl"}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert agent binding");

    let (status, response) =
        get_current_turn_by_client_session(state, "pi", "pi_session_123").await;

    assert_eq!(status, StatusCode::OK, "{response:?}");
    let current_turn = &response["data"]["current_turn"];
    assert_eq!(current_turn["session_id"], "sess_current");
    assert_eq!(current_turn["turn_id"], "turn_current");
    assert_eq!(current_turn["client_type"], "pi");
    assert_eq!(current_turn["client_session_key"], "pi_session_123");
    assert_eq!(current_turn["runtime_instance_id"], "rtinst_current");
    assert_eq!(
        current_turn["internal_event_url"],
        "http://127.0.0.1:18080/internal/v1/events"
    );
    assert_eq!(current_turn["runtime_metadata"]["launch_cwd"], "/repo");
    assert_eq!(
        current_turn["binding_metadata"]["transcript_path"],
        "/tmp/pi.jsonl"
    );
}

#[tokio::test]
async fn internal_agent_binding_current_turn_ignores_a_terminal_sticky_branch_leaf() {
    let (state, _app) = test_state().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES ('sess_idle', 'pi', 'idle', 'turn_completed', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, metadata)
           VALUES ('turn_completed', 'sess_idle', 'completed', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert terminal branch leaf");
    sqlx::query(
        r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id)
           VALUES ('sess_idle', 'pi_tui', 'rtinst_idle')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert runtime binding");
    sqlx::query(
        r#"INSERT INTO agent_bindings (id, session_id, client_type, launch_cwd, client_session_key, metadata)
           VALUES ('binding_idle', 'sess_idle', 'pi', '/repo', 'pi_idle', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert agent binding");

    let (status, response) = get_current_turn_by_client_session(state, "pi", "pi_idle").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{response:?}");
    assert_eq!(response["error"]["code"], "not_found");
}
