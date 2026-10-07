use pontia_storage_sqlite::connect_sqlite;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};

async fn legacy_pool() -> (SqlitePool, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("migration.db").display()
    ))
    .await
    .unwrap();
    let migrator = sqlx::migrate!("./migrations");
    for migration in migrator.iter().filter(|migration| migration.version < 28) {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    (pool, root)
}

#[tokio::test]
async fn migration_preserves_sessions_native_identity_history_and_runtime_references() {
    let (pool, _root) = legacy_pool().await;
    let schemas: Vec<(String, String)> = sqlx::query_as("SELECT name, sql FROM sqlite_master WHERE name IN ('sessions','agent_bindings','codex_tui_bindings') ORDER BY name").fetch_all(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO sessions(session_id, client_type, state) VALUES ('pi', 'pi', 'busy'), ('codex', 'codex', 'idle');
        INSERT INTO agent_bindings(id, session_id, client_type, launch_cwd, client_session_key) VALUES ('binding', 'pi', 'pi', '/workspace', 'native');
        INSERT INTO runtime_bindings(session_id, runtime_kind, runtime_instance_id, binding_state, start_command, tmux_socket_path, tmux_pane_id)
            VALUES ('pi', 'pi_tui', 'launch-new', 'confirmed', 'pi --approve', '/tmp/test.sock', '%1'), ('codex', 'codex_app_server', 'codex-launch', 'confirmed', NULL, NULL, NULL);
        INSERT INTO codex_tui_bindings(owner_session_id,target_session_id,runtime_instance_id) VALUES ('codex','codex','frozen-tui');
        INSERT INTO events(event_id, session_id, client_type, event_type, source, occurred_at, payload)
            VALUES ('old-ready','pi','pi','session.ready','agent_client','2026-01-01T00:00:00Z','{"runtime_instance_id":"launch-old","preserved":"old"}'),
                   ('new-ready','pi','pi','session.ready','agent_client','2026-02-01T00:00:00Z','{"runtime_instance_id":"launch-new","preserved":"new"}');
        INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary,required_runtime_instance_id) VALUES ('message','pi','queued','after_idle','input','launch-old');
        INSERT INTO workflows(workflow_id,title,cwd,state) VALUES ('workflow','title','/workspace','running');
        INSERT INTO workflow_nodes(node_id,workflow_id,title,instructions,output,session_id,submitted_runtime_instance_id) VALUES ('node','workflow','title','instructions','output','pi','launch-old');
        INSERT INTO turns(turn_id,session_id,state) VALUES ('turn','pi','running');
        INSERT INTO workflow_patches(patch_id,workflow_id,requesting_node_id,requesting_session_id,requesting_turn_id,requesting_runtime_instance_id,replanner_creation_token,base_revision,state,request_document_ref,request_size_bytes)
            VALUES ('patch','workflow','node','pi','turn','launch-old','token',1,'requested','request.md',1);
        INSERT INTO workflow_events(event_id,workflow_id,event_type,payload,sequence) VALUES ('failure','workflow','workflow.failed','{"session_id":"pi","runtime_instance_id":"launch-old"}',1);
        INSERT INTO workflow_recoveries(recovery_id,workflow_id,failure_event_id,exit_event_id,node_id,session_id,message_id,state,runtime_instance_id)
            VALUES ('recovery','workflow','failure','old-ready','node','pi','recovery-message','completed','launch-new');
    "#).execute(&pool).await.unwrap();
    let fingerprint = json!({"boot_id":"boot", "pane_pid":11, "pane_start_time_ticks":12, "agent_pid":13, "agent_start_time_ticks":14, "agent_comm":"pi", "agent_argv0":"pi"});
    sqlx::query("UPDATE runtime_bindings SET process_fingerprint=? WHERE session_id='pi'")
        .bind(fingerprint.to_string())
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0028_session_runtimes.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let row = sqlx::query("SELECT * FROM session_runtimes WHERE session_id='pi'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let id: String = row.get("runtime_id");
    assert_ne!(id, "launch-new");
    assert_eq!(row.get::<String, _>("role"), "tui");
    assert_eq!(row.get::<String, _>("state"), "running");
    assert_eq!(row.get::<String, _>("start_command"), "pi --approve");
    let converted: Value =
        serde_json::from_str(&row.get::<String, _>("process_fingerprint")).unwrap();
    assert_eq!(converted["agent_pid"], 13);
    assert_eq!(converted["tmux_socket_path"], "/tmp/test.sock");
    assert_eq!(converted["tmux_pane_id"], "%1");
    for query in [
        "SELECT required_runtime_id FROM inbox_messages WHERE message_id='message'",
        "SELECT submitted_runtime_id FROM workflow_nodes WHERE node_id='node'",
        "SELECT requesting_runtime_id FROM workflow_patches WHERE patch_id='patch'",
        "SELECT runtime_id FROM workflow_recoveries WHERE recovery_id='recovery'",
        "SELECT json_extract(payload,'$.runtime_id') FROM workflow_events WHERE event_id='failure'",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, String>(query)
                .fetch_one(&pool)
                .await
                .unwrap(),
            id
        );
    }
    let payloads: Vec<String> = sqlx::query_scalar("SELECT payload FROM events ORDER BY event_id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(payloads.len(), 2);
    for payload in payloads {
        let payload: Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["runtime_id"], id);
        assert!(payload.get("runtime_instance_id").is_none());
        assert!(payload.get("preserved").is_some());
    }
    let after: Vec<(String,String)> = sqlx::query_as("SELECT name,sql FROM sqlite_master WHERE name IN ('sessions','agent_bindings','codex_tui_bindings') ORDER BY name").fetch_all(&pool).await.unwrap();
    assert_eq!(schemas, after);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT runtime_instance_id FROM codex_tui_bindings")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "frozen-tui"
    );
    assert!(
        !sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='runtime_bindings')"
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_bindings WHERE client_session_key='native'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn migration_does_not_fabricate_missing_process_evidence() {
    let (pool, _root) = legacy_pool().await;
    sqlx::raw_sql("INSERT INTO sessions(session_id,client_type,state) VALUES ('incomplete','pi','idle'), ('missing','pi','idle'); INSERT INTO runtime_bindings(session_id,runtime_kind,binding_state,tmux_socket_path,tmux_pane_id,process_fingerprint) VALUES ('incomplete','pi_tui','confirmed','/unused/socket','%1','{}'), ('missing','pi_tui','confirmed','/unused/socket','%2',NULL)").execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0028_session_runtimes.sql"))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let evidence: Vec<Option<String>> =
        sqlx::query_scalar("SELECT process_fingerprint FROM session_runtimes")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(evidence, vec![None, None]);
}
