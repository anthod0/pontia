use pontia_agent_control::ControlQueryService;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use pontia_workflow::repository::SqliteWorkflowRepository;

#[tokio::test]
async fn replay_and_restart_use_only_application_storage_with_a_separate_control_database() {
    let root = tempfile::tempdir().unwrap();
    let control = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("control.db").display()
    ))
    .await
    .unwrap();
    let application = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("application.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&control).await.unwrap();
    run_migrations(&application).await.unwrap();
    sqlx::raw_sql("INSERT INTO sessions(session_id,client_type,state,application_id,application_ref) VALUES ('s','generic','idle','workflow','node:n');
        INSERT INTO events(event_id,session_id,turn_id,source,client_type,event_type,occurred_at,payload) VALUES ('start','s','t','agent_client','generic','turn.started','2026-01-01','{\"runtime_id\":\"r\"}'),('end','s','t','agent_client','generic','turn.completed','2026-01-01','{}');")
        .execute(&control).await.unwrap();
    let repository = SqliteWorkflowRepository::new(
        application.clone(),
        ControlQueryService::new(control.clone()),
    );
    let terminal = repository
        .latest_workflow_terminal_event("s", Some("r"), Some("t"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(terminal.event_id, "end");
    drop(repository);
    let restarted =
        SqliteWorkflowRepository::new(application.clone(), ControlQueryService::new(control));
    restarted.synchronize().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workflow_agent_events")
            .fetch_one(&application)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
            .fetch_one(&application)
            .await
            .unwrap(),
        0
    );
    // Replaying a previously committed batch must not duplicate facts.
    sqlx::query("UPDATE workflow_event_cursor SET position=0")
        .execute(&application)
        .await
        .unwrap();
    restarted.synchronize().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workflow_agent_events")
            .fetch_one(&application)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        restarted
            .latest_workflow_terminal_event("s", Some("r"), None)
            .await
            .unwrap()
            .unwrap()
            .event_id,
        "end"
    );
}

#[tokio::test]
async fn upgrade_preserves_owned_sessions_history_and_runtime_environment() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("upgrade.db").display()
    ))
    .await
    .unwrap();
    for migration in sqlx::migrate!("../pontia-storage-sqlite/migrations")
        .iter()
        .filter(|m| m.version < 39)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO sessions(session_id,client_type,state) VALUES ('s','generic','busy');
        INSERT INTO turns(turn_id,session_id,state) VALUES ('t','s','running');
        INSERT INTO workflows(workflow_id,title,cwd,state) VALUES ('w','Title','/workspace','running');
        INSERT INTO workflow_nodes(node_id,workflow_id,title,instructions,output,session_id) VALUES ('n','w','Node','Work','result.md','s');
        INSERT INTO events(event_id,session_id,turn_id,source,client_type,event_type,occurred_at,payload) VALUES ('event','s','t','agent_client','generic','turn.started','2026-01-01','{\"runtime_id\":\"r\"}');
        INSERT INTO workflow_patches(patch_id,workflow_id,requesting_node_id,requesting_session_id,requesting_turn_id,requesting_runtime_id,replanner_creation_token,base_revision,state,request_document_ref,request_size_bytes) VALUES ('p','w','n','s','t','r','token',1,'requested','problem.md',1);")
        .execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!(
        "../../pontia-storage-sqlite/migrations/0039_control_plane_applications.sql"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let owned: (String,String,String) = sqlx::query_as("SELECT application_id,application_ref,runtime_environment FROM sessions WHERE session_id='s'").fetch_one(&pool).await.unwrap();
    assert_eq!(owned.0, "workflow");
    assert_eq!(owned.1, "node:n");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&owned.2).unwrap()["PONTIA_WORKFLOW_ID"],
        "w"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT event_id FROM workflow_agent_events")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "event"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT requesting_turn_id FROM workflow_patches WHERE patch_id='p'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "t"
    );
    sqlx::query("DELETE FROM turns WHERE turn_id='t'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty()
    );
}
