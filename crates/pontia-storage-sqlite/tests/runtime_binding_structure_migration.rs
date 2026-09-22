use pontia_storage_sqlite::connect_sqlite;
use serde_json::Value;

#[tokio::test]
async fn migration_splits_runtime_binding_metadata_without_losing_process_or_turn_context() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("runtime-binding-structure-migration.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");

    sqlx::raw_sql(include_str!("../migrations/0001_init.sql"))
        .execute(&pool)
        .await
        .expect("initialize baseline schema");
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, metadata)
           VALUES ('sess_migrated', 'pi', 'idle', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO runtime_bindings (
               session_id, runtime_kind, runtime_instance_id, start_command, launch_cwd,
               last_seen_at, tmux_socket_path, tmux_pane_id, metadata
           ) VALUES (
               'sess_migrated', 'tmux', 'rtinst_migrated', 'pi --approve', '/workspace',
               '2026-07-01T00:01:00Z', '/tmp/tmux.sock', '%4', ?
           )"#,
    )
    .bind(
        serde_json::json!({
            "binding_confirmed": true,
            "internal_event_url": "http://127.0.0.1/internal/v1/events",
            "started_at": "2026-07-01T00:00:00Z",
            "restart_count": 2,
            "tmux_process_fingerprint": {"agent_pid": 42, "boot_id": "boot"},
            "capabilities": {"accept_task": true},
            "log_dir": "/state",
            "runtime_log": "/state/runtime.log",
            "pi_hook_log": "/state/pi-hook.log",
            "claude_hook_log": "/state/claude-hook.log",
            "tmux": {"session_name": "pontia"},
            "pending_current_turn": {
                "session_id": "sess_migrated",
                "runtime_instance_id": "rtinst_migrated",
                "client_type": "pi",
                "input": "hello"
            }
        })
        .to_string(),
    )
    .execute(&pool)
    .await
    .expect("insert legacy runtime binding");

    sqlx::raw_sql(include_str!(
        "../migrations/0013_structure_runtime_bindings.sql"
    ))
    .execute(&pool)
    .await
    .expect("structure runtime binding");

    let row: (String, String, String, String, String) = sqlx::query_as(
        r#"SELECT binding_state, process_fingerprint, capabilities, diagnostics, adapter_details
           FROM runtime_bindings WHERE session_id = 'sess_migrated'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("migrated runtime binding");
    assert_eq!(row.0, "confirmed");
    assert_eq!(
        serde_json::from_str::<Value>(&row.1).unwrap()["agent_pid"],
        42
    );
    assert_eq!(
        serde_json::from_str::<Value>(&row.2).unwrap()["accept_task"],
        true
    );
    assert_eq!(
        serde_json::from_str::<Value>(&row.3).unwrap()["runtime_log"],
        "/state/runtime.log"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&row.4).unwrap()["tmux"]["session_name"],
        "pontia"
    );
    let diagnostics = serde_json::from_str::<Value>(&row.3).unwrap();
    assert_eq!(diagnostics["pi_hook_log"], "/state/pi-hook.log");
    assert_eq!(diagnostics["claude_hook_log"], "/state/claude-hook.log");

    sqlx::raw_sql(include_str!(
        "../migrations/0020_remove_claude_runtime_diagnostics.sql"
    ))
    .execute(&pool)
    .await
    .expect("remove Claude runtime diagnostics");

    let diagnostics: String = sqlx::query_scalar(
        "SELECT diagnostics FROM runtime_bindings WHERE session_id = 'sess_migrated'",
    )
    .fetch_one(&pool)
    .await
    .expect("load cleaned runtime diagnostics");
    let diagnostics = serde_json::from_str::<Value>(&diagnostics).unwrap();
    assert_eq!(diagnostics["pi_hook_log"], "/state/pi-hook.log");
    assert!(diagnostics.get("claude_hook_log").is_none());

    let pending: (String, String, String) = sqlx::query_as(
        r#"SELECT runtime_instance_id, client_type, payload
           FROM pending_turn_contexts WHERE session_id = 'sess_migrated'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("migrated pending context");
    assert_eq!(pending.0, "rtinst_migrated");
    assert_eq!(pending.1, "pi");
    assert_eq!(
        serde_json::from_str::<Value>(&pending.2).unwrap()["input"],
        "hello"
    );
}
