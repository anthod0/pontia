use pontia_application::{
    AppState, RuntimeReadinessService,
    client_contract::{RuntimeBindingBehavior, test_registration},
    clients::ClientRegistry,
    runtime::ControlTarget,
};
use pontia_core::{Error, domain::EventType};
use serde_json::json;

async fn fixture(role: Option<&'static str>) -> (AppState, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&db).await.unwrap();
    let mut registration = test_registration();
    let mut spec = registration.spec.clone();
    spec.adapter.control_runtime_role = role;
    spec.adapter.runtime_binding = RuntimeBindingBehavior::Named {
        runtime_kind: "test",
    };
    registration.spec = Box::leak(Box::new(spec));
    let mut clients = ClientRegistry::default();
    clients.register(registration);
    let app = AppState::builder(db.clone(), root.path().into())
        .clients(clients)
        .build();
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('session','generic','idle')",
    )
    .execute(&db)
    .await
    .unwrap();
    (app, root)
}

async fn runtime(app: &AppState, id: &str, role: &str) {
    sqlx::query("INSERT INTO session_runtimes(session_id,runtime_id,role,state,created_at) VALUES ('session',?,?,'running','2026-01-01T00:00:00Z')")
        .bind(id).bind(role).execute(&app.db()).await.unwrap();
}

#[tokio::test]
async fn only_the_registered_role_is_controlled_and_ready_among_other_runtimes() {
    let (app, _root) = fixture(Some("owned")).await;
    runtime(&app, "worker", "worker").await;
    runtime(&app, "interface", "interface").await;
    runtime(&app, "tui", "tui").await;
    runtime(&app, "control", "owned").await;

    let target = ControlTarget::resolve(&app.db(), &app.clients(), "session", None)
        .await
        .unwrap();
    assert_eq!(target.runtime_id.as_deref(), Some("control"));
    target.validate(&app.db()).await.unwrap();
    let readiness =
        RuntimeReadinessService::new(app.db(), app.clients().control_runtime_role("generic"));
    assert!(
        readiness
            .is_ready("session", "generic", "control")
            .await
            .unwrap()
    );
    for id in ["worker", "interface", "tui"] {
        assert!(!readiness.is_ready("session", "generic", id).await.unwrap());
        assert!(matches!(
            ControlTarget::resolve(&app.db(), &app.clients(), "session", Some(id)).await,
            Err(Error::StateConflict(_))
        ));
    }
}

#[tokio::test]
async fn an_undeclared_runtime_cannot_be_a_control_target_or_supply_ready_facts() {
    let (app, _root) = fixture(Some("owned")).await;
    runtime(&app, "worker", "worker").await;
    let target = ControlTarget::resolve(&app.db(), &app.clients(), "session", None)
        .await
        .unwrap();
    assert!(target.runtime_id.is_none());
    assert!(matches!(
        ControlTarget::resolve(&app.db(), &app.clients(), "session", Some("worker")).await,
        Err(Error::StateConflict(_))
    ));
    assert!(
        app.event_ingest_service()
            .report_client_fact(
                "session",
                Some("worker"),
                EventType::SessionReady,
                json!({})
            )
            .await
            .is_err()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.ready'")
            .fetch_one(&app.db())
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn an_undeclared_runtime_cannot_exit_the_session_when_control_runtime_is_missing() {
    let (app, _root) = fixture(Some("owned")).await;
    runtime(&app, "worker", "worker").await;
    assert!(
        app.event_ingest_service()
            .report_client_fact(
                "session",
                Some("worker"),
                EventType::SessionExited,
                json!({"reason":"worker_exited"})
            )
            .await
            .is_err()
    );
    let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id='session'")
        .fetch_one(&app.db())
        .await
        .unwrap();
    assert_eq!(state, "idle");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.exited'")
            .fetch_one(&app.db())
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn a_client_without_a_declared_role_has_no_control_runtime() {
    let (app, _root) = fixture(None).await;
    runtime(&app, "tui", "tui").await;
    runtime(&app, "worker", "worker").await;
    let target = ControlTarget::resolve(&app.db(), &app.clients(), "session", None)
        .await
        .unwrap();
    assert!(target.runtime_id.is_none());
    let readiness = RuntimeReadinessService::new(app.db(), None);
    assert!(
        !readiness
            .is_ready("session", "generic", "tui")
            .await
            .unwrap()
    );
    assert!(
        !readiness
            .is_ready("session", "generic", "worker")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn captured_control_target_can_be_validated_while_session_writes_are_locked() {
    let (app, _root) = fixture(Some("owned")).await;
    runtime(&app, "control", "owned").await;
    let target = ControlTarget::resolve(&app.db(), &app.clients(), "session", None)
        .await
        .unwrap();
    let mut tx = app.db().begin().await.unwrap();
    sqlx::query("UPDATE sessions SET state=state WHERE session_id='session'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        target.validate(&app.db()),
    )
    .await
    .unwrap()
    .unwrap();
    tx.rollback().await.unwrap();
}
