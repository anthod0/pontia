mod support;
use std::time::Duration;

use pontia_core::domain::{EventSource, EventType, ReportedEvent};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use sqlx::SqlitePool;

use pontia_agent_control::{AgentControl, EventIngestService};

async fn setup() -> (SqlitePool, tempfile::TempDir, AgentControl) {
    let root = tempfile::Builder::new().prefix("pd-").tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("test.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sessions (session_id,client_type,state) VALUES ('sess_pi','pi','starting')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, tmux_socket_path, tmux_pane_id, role, state, created_at) VALUES ('sess_pi', 'rtinst_pi', '/unused/tmux', '%1', 'pi_tui', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#).execute(&pool).await.unwrap();
    sqlx::query("UPDATE session_runtimes SET state='starting' WHERE session_id='sess_pi'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO agent_bindings (id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding_pi','sess_pi','pi','/unused','native_pi','{}')").execute(&pool).await.unwrap();
    let state = AgentControl::builder(pool.clone(), root.path().into())
        .clients(support::clients())
        .build();
    (pool, root, state)
}

async fn ready(pool: &SqlitePool) {
    EventIngestService::for_projection_tests(pool.clone())
        .with_clients(support::clients())
        .ingest_reported_event(ReportedEvent::new(
            "evt_ready".into(),
            "sess_pi".into(),
            None,
            EventSource::AgentClient,
            "pi".into(),
            EventType::SessionReady,
            json!({"runtime_id":"rtinst_pi"}),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn pi_input_waits_for_ready_uses_socket_and_leaves_lifecycle_to_client() {
    let (pool, _root, state) = setup().await;
    let (peer, mut requests) = support::attach(&state, "sess_pi", "rtinst_pi", "native_pi").await;
    let server = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        assert_eq!(request.method, "submit");
        assert_eq!(request.params["input"], "hello after ready");
        peer.reply(request.id, json!({"accepted":true}))
            .await
            .unwrap();
    });
    let service = state.turn_commands();
    let dispatch = tokio::spawn(async move {
        service
            .start_turn("sess_pi", "hello after ready".into(), json!({}))
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!dispatch.is_finished());
    ready(&pool).await;
    assert!(
        tokio::time::timeout(Duration::from_secs(2), dispatch)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .is_none()
    );
    server.await.unwrap();
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(turns, 0);
}

#[tokio::test]
async fn missing_pi_connection_rejects_delivery_without_creating_a_turn() {
    let (pool, _root, state) = setup().await;
    ready(&pool).await;
    let error = state
        .turn_commands()
        .start_turn("sess_pi", "not delivered".into(), json!({}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no current Client connection"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn conditional_exit_refuses_unsettled_control_state_before_contacting_the_client() {
    let (pool, _root, app) = setup().await;
    let commands = app.session_commands();
    // No peer is attached, so reaching the client would return an error.
    assert!(!commands.exit_if_idle("sess_pi").await.unwrap());
    ready(&pool).await;
    sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES ('turn','sess_pi','queued')")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!commands.exit_if_idle("sess_pi").await.unwrap());
    sqlx::query("UPDATE turns SET state='running' WHERE turn_id='turn'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!commands.exit_if_idle("sess_pi").await.unwrap());
    sqlx::query("UPDATE turns SET state='completed' WHERE turn_id='turn'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO inbox_messages(message_id,session_id,input_summary) VALUES ('next','sess_pi','next')")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!commands.exit_if_idle("sess_pi").await.unwrap());
}

#[tokio::test]
async fn conditional_exit_uses_the_native_idle_decision_and_waits_for_exit_confirmation() {
    let (pool, _root, state) = setup().await;
    ready(&pool).await;
    let (peer, mut requests) = support::attach(&state, "sess_pi", "rtinst_pi", "native_pi").await;
    let events = state.event_ingest_service();
    let server = tokio::spawn(async move {
        for accepted in [false, true] {
            let request = requests.recv().await.unwrap();
            // Native execution can resume after the daemon's idle snapshot.
            assert_eq!(request.method, "shutdown_if_idle");
            peer.reply(request.id, json!({"accepted":accepted}))
                .await
                .unwrap();
            if accepted {
                events
                    .report_client_fact(
                        "sess_pi",
                        Some("rtinst_pi"),
                        EventType::SessionExited,
                        json!({"reason":"idle shutdown"}),
                    )
                    .await
                    .unwrap();
            }
        }
    });
    assert!(
        !state
            .session_commands()
            .exit_if_idle("sess_pi")
            .await
            .unwrap()
    );
    assert_eq!(
        state
            .queries()
            .session_snapshot("sess_pi")
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle"
    );
    assert!(
        state
            .session_commands()
            .exit_if_idle("sess_pi")
            .await
            .unwrap()
    );
    assert_eq!(
        state
            .queries()
            .session_snapshot("sess_pi")
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn disconnected_runtime_can_be_explicitly_closed() {
    let (pool, _root, app) = setup().await;
    ready(&pool).await;
    let commands = app.session_commands();
    assert!(matches!(
        commands.terminate_session("sess_pi").await,
        Err(pontia_core::Error::CapabilityUnavailable(_))
    ));
    let mut agent = tokio::process::Command::new("sleep")
        .arg("60")
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let fingerprint = pontia_runtime::ProcessFingerprint::capture(agent.id().unwrap()).unwrap();
    sqlx::query("UPDATE session_runtimes SET process_fingerprint=? WHERE runtime_id='rtinst_pi'")
        .bind(serde_json::to_string(&fingerprint).unwrap())
        .execute(&pool)
        .await
        .unwrap();

    assert!(matches!(
        commands.exit_if_idle("sess_pi").await,
        Err(pontia_core::Error::CapabilityUnavailable(_))
    ));
    commands.terminate_session("sess_pi").await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), agent.wait())
        .await
        .unwrap()
        .unwrap();
    app.runtime_observer()
        .observe_session("sess_pi")
        .await
        .unwrap();
    let session = app.queries().get_session("sess_pi").await.unwrap().unwrap();
    assert_eq!(session.state, "exited");
    assert_eq!(session.runtimes[0].state, "exited");
}

#[tokio::test]
async fn running_runtime_exposes_its_current_connection_independently_of_session_state() {
    let (pool, _root, app) = setup().await;
    ready(&pool).await;
    let view = || async {
        let session = app.queries().get_session("sess_pi").await.unwrap().unwrap();
        assert_eq!(session.state, "idle");
        assert_eq!(session.runtimes[0].state, "running");
        session.control_connected
    };
    assert_eq!(view().await, Some(false));
    let (peer, _requests) = support::attach(&app, "sess_pi", "rtinst_pi", "native_pi").await;
    assert_eq!(view().await, Some(true));
    peer.close();
    tokio::time::timeout(Duration::from_secs(5), peer.closed())
        .await
        .unwrap();
    assert_eq!(view().await, Some(false));
}
