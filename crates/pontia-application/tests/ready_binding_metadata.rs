use pontia_application::{
    AgentBindingService, AppState, UpsertAgentBindingRequest,
    client_contract::{ClientSessionIdentityBehavior, test_registration},
    clients::ClientRegistry,
};
use pontia_core::domain::EventType;
use serde_json::{Value, json};

async fn fixture() -> (AppState, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&db).await.unwrap();
    let mut registration = test_registration();
    let mut spec = registration.spec.clone();
    spec.adapter.client_session_identity = ClientSessionIdentityBehavior::RequiredOnReady;
    registration.spec = Box::leak(Box::new(spec));
    let mut clients = ClientRegistry::default();
    clients.register(registration);
    let app = AppState::builder(db.clone(), root.path().into())
        .clients(clients)
        .build();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state,workspace_ref) VALUES ('session','generic','idle',?)").bind(root.path().to_str().unwrap()).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO session_runtimes(session_id,runtime_id,role,state,created_at) VALUES ('session','runtime','tui','running','2026-01-01T00:00:00Z')").execute(&db).await.unwrap();
    AgentBindingService::new(db)
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: "session".into(),
            client_type: "generic".into(),
            launch_cwd: root.path().display().to_string(),
            client_session_key: "native".into(),
            client_session_file: None,
            metadata: json!({"stale":true,"client_cwd":"/old","agent_info":{"name":"old"}}),
        })
        .await
        .unwrap();
    (app, root)
}

async fn metadata(app: &AppState) -> Value {
    AgentBindingService::new(app.db())
        .binding_for_session("session")
        .await
        .unwrap()
        .unwrap()
        .metadata
}

async fn ready(app: &AppState, fields: Value) -> pontia_core::Result<()> {
    let mut payload = json!({"client_session_key":"native"});
    payload
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    app.event_ingest_service()
        .report_client_fact("session", Some("runtime"), EventType::SessionReady, payload)
        .await?;
    Ok(())
}

#[tokio::test]
async fn ready_replaces_metadata_with_the_complete_client_snapshot() {
    let (app, _root) = fixture().await;
    let snapshot = json!({"agent_info":{"name":"new","version":"2"}});
    ready(
        &app,
        json!({"binding_metadata":snapshot,"client_cwd":"/not-part-of-the-explicit-snapshot"}),
    )
    .await
    .unwrap();
    assert_eq!(metadata(&app).await, snapshot);
}

#[tokio::test]
async fn empty_ready_snapshot_clears_all_previous_metadata() {
    let (app, _root) = fixture().await;
    ready(&app, json!({"binding_metadata":{}})).await.unwrap();
    assert_eq!(metadata(&app).await, json!({}));
}

#[tokio::test]
async fn ready_without_an_explicit_snapshot_keeps_existing_diagnostic_overwrite_semantics() {
    let (app, _root) = fixture().await;
    ready(&app, json!({"client_session_dir":"/new"}))
        .await
        .unwrap();
    assert_eq!(metadata(&app).await, json!({"client_session_dir":"/new"}));
    ready(&app, json!({})).await.unwrap();
    assert_eq!(metadata(&app).await, json!({}));
}

#[tokio::test]
async fn invalid_ready_metadata_is_rejected_without_overwriting_the_binding() {
    let (app, _root) = fixture().await;
    let before = metadata(&app).await;
    assert!(ready(&app, json!({"binding_metadata":[]})).await.is_err());
    assert_eq!(metadata(&app).await, before);
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.ready'")
            .fetch_one(&app.db())
            .await
            .unwrap();
    assert_eq!(count, 0);
}
