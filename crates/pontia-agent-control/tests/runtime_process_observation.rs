use std::{
    process::{Command, Stdio},
    time::Duration,
};

use pontia_agent_control::AgentControl;
use pontia_runtime::ProcessFingerprint;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

const OBSERVED_AGENT: pontia_agent_control::client_contract::AgentClientAdapter =
    pontia_agent_control::client_contract::AgentClientAdapter {
        lifecycle: pontia_agent_control::client_contract::SessionLifecycleBehavior {
            process_observation: Some(
                pontia_agent_control::client_contract::ProcessObservationBehavior {
                    role: "generic_control",
                    observe_starting: false,
                    exit_event: pontia_agent_control::PontiaEventType::SessionExited,
                    exit_reason: "agent_process_fingerprint_missing",
                },
            ),
            ..pontia_agent_control::client_contract::SessionLifecycleBehavior::DEFAULT
        },
        ..pontia_agent_control::client_contract::TEST_SPEC.adapter
    };

#[tokio::test]
async fn missing_bound_agent_process_projects_session_exited_after_confirmation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut child = Command::new("sleep")
        .arg("60")
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let fingerprint = ProcessFingerprint::capture(child.id()).unwrap();
    let database_url = format!(
        "sqlite://{}?mode=rwc",
        temp.path().join("test.db").display()
    );
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_observed', 'pi', 'idle')",
    )
    .execute(&db)
    .await
    .expect("insert session");
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, process_fingerprint, role, state, created_at) VALUES (?, ?, ?, 'generic_control', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#)
.bind("sess_observed")
.bind("rtinst_observed")
.bind(json!(fingerprint).to_string())
    .execute(&db)
    .await
    .expect("insert binding");

    child.kill().unwrap();
    child.wait().unwrap();
    wait_for_process_exit(&fingerprint).await;

    AgentControl::builder(db.clone(), temp.path().into())
        .clients({
            let mut clients = pontia_agent_control::clients::ClientRegistry::default();
            static SPEC: pontia_agent_control::client_contract::AgentClientSpec =
                pontia_agent_control::client_contract::AgentClientSpec {
                    client_type: "pi",
                    adapter: OBSERVED_AGENT,
                    ..pontia_agent_control::client_contract::TEST_SPEC
                };
            let mut entry = pontia_agent_control::client_contract::test_registration();
            entry.spec = &SPEC;
            clients.register(entry);
            clients
        })
        .build()
        .runtime_observer()
        .sweep_active_processes()
        .await
        .expect("sweep runtime bindings");

    let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
        .bind("sess_observed")
        .fetch_one(&db)
        .await
        .expect("load session state");
    assert_eq!(state, "exited");

    let source: String = sqlx::query_scalar(
        "SELECT source FROM events WHERE session_id = ? AND event_type = 'session.exited'",
    )
    .bind("sess_observed")
    .fetch_one(&db)
    .await
    .expect("load exit event");
    assert_eq!(source, "runtime_manager");
}

#[tokio::test]
async fn active_session_without_a_fingerprint_remains_unknown() {
    let temp = tempfile::tempdir().expect("tempdir");
    let database_url = format!(
        "sqlite://{}?mode=rwc",
        temp.path().join("missing-fingerprint.db").display()
    );
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_without_fingerprint', 'pi', 'idle')",
    )
    .execute(&db)
    .await
    .expect("insert session");
    sqlx::query(r#"INSERT INTO session_runtimes(session_id, runtime_id, tmux_socket_path, tmux_pane_id, role, state, created_at) VALUES (?, ?, ?, ?, 'generic_control', 'running', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#)
.bind("sess_without_fingerprint")
.bind("rtinst_without_fingerprint")
.bind(temp.path().join("missing-tmux-socket").to_str().unwrap())
.bind("%404")
    .execute(&db)
    .await
    .expect("insert binding");

    AgentControl::builder(db.clone(), temp.path().into())
        .clients({
            let mut clients = pontia_agent_control::clients::ClientRegistry::default();
            static SPEC: pontia_agent_control::client_contract::AgentClientSpec =
                pontia_agent_control::client_contract::AgentClientSpec {
                    client_type: "pi",
                    adapter: OBSERVED_AGENT,
                    ..pontia_agent_control::client_contract::TEST_SPEC
                };
            let mut entry = pontia_agent_control::client_contract::test_registration();
            entry.spec = &SPEC;
            clients.register(entry);
            clients
        })
        .build()
        .runtime_observer()
        .sweep_active_processes()
        .await
        .expect("sweep runtime bindings");

    let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
        .bind("sess_without_fingerprint")
        .fetch_one(&db)
        .await
        .expect("load session state");
    assert_eq!(state, "idle");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM events WHERE event_type='session.exited'"
        )
        .fetch_one(&db)
        .await
        .unwrap(),
        0
    );
}

async fn wait_for_process_exit(fingerprint: &pontia_runtime::ProcessFingerprint) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while fingerprint.observe() != pontia_runtime::ProcessObservation::Exited {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("test agent process must be confirmed exited before observation");
}
