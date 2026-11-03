use super::*;

async fn read_environment(h: &Harness, session: &str) {
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.release("release-1");
    h.wait_turn(session, &turn.turn_id, "completed").await;
}

#[tokio::test]
async fn commands_keep_the_session_environment_after_resume() {
    let h = Harness::new(&[
        ("ACP_READ_SESSION_ENV", "1"),
        ("PONTIA_SESSION_ID", "wrong-configured-session"),
    ])
    .await;
    let session = h.create("first").await.unwrap();
    read_environment(&h, &session).await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    h.app
        .session_commands()
        .resume_session(&session, h.root.path())
        .await
        .unwrap();
    read_environment(&h, &session).await;
    let values: Vec<_> = h
        .wire()
        .into_iter()
        .filter_map(|frame| frame.get("session_environment").cloned())
        .collect();
    assert_eq!(values, [json!(session), json!(session)]);
    h.stop().await;
}

#[tokio::test]
async fn commands_use_their_own_session_environment_with_a_shared_agent_and_workspace() {
    let h = Harness::new(&[
        ("ACP_READ_SESSION_ENV", "1"),
        ("ACP_UNIQUE_ID", "1"),
        ("PONTIA_SESSION_ID", "wrong-configured-session"),
    ])
    .await;
    let a = h.create("first").await.unwrap();
    let b = h.create("first").await.unwrap();
    tokio::join!(read_environment(&h, &a), read_environment(&h, &b));
    let readings: Vec<_> = h
        .wire()
        .into_iter()
        .filter(|frame| frame.get("session_environment").is_some())
        .collect();
    assert_eq!(readings.len(), 2);
    for session in [&a, &b] {
        let binding = pontia_application::AgentBindingService::new(h.app.db())
            .binding_for_session(session)
            .await
            .unwrap()
            .unwrap();
        let [_, native] = identity::decode(&binding.client_session_key).unwrap();
        let reading = readings
            .iter()
            .find(|reading| reading["native"] == native)
            .unwrap();
        assert_eq!(reading["session_environment"], *session);
    }
    h.stop().await;
}
