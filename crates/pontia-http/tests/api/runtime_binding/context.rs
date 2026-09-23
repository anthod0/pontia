use super::{StatusCode, get_session_context_by_client_session, test_state};
#[tokio::test]
async fn pi_session_context_returns_stable_runtime_without_an_active_turn() {
    let (state, _app) = test_state().await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES ('sess_context', 'pi', 'idle', NULL, '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id)
           VALUES ('sess_context', 'pi_tui', 'rtinst_stable')"#,
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
}
