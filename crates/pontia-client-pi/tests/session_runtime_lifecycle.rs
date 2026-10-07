mod support;
use pontia_application::{AppState, PontiaEvent, PontiaEventSource, PontiaEventType, ReportedFact};
use pontia_core::domain::EventType;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};

async fn fixture() -> (AppState, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("lifecycle.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    let app = AppState::builder(pool, root.path().into())
        .clients(support::clients())
        .build();
    app.event_ingest_service()
        .ingest_pontia_event(PontiaEvent::new(
            "session",
            None,
            PontiaEventSource::ExternalApi,
            "pi",
            PontiaEventType::SessionCreated,
            json!({"workspace":root.path()}),
        ))
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET workspace_ref=? WHERE session_id='session'")
        .bind(root.path().display().to_string())
        .execute(&app.db())
        .await
        .unwrap();
    app.event_ingest_service()
        .ingest_pontia_event(PontiaEvent::new(
            "session",
            None,
            PontiaEventSource::ExternalApi,
            "pi",
            PontiaEventType::SessionStarting,
            json!({"runtime_id":"runtime"}),
        ))
        .await
        .unwrap();
    (app, root)
}

async fn states(app: &AppState) -> (String, String) {
    sqlx::query_as("SELECT s.state,r.state FROM sessions s JOIN session_runtimes r USING(session_id) WHERE runtime_id='runtime'").fetch_one(&app.db()).await.unwrap()
}

async fn ready(
    app: &AppState,
) -> Result<pontia_application::EventIngestResult, pontia_application::EventReportError> {
    app.event_ingest_service()
        .report_fact(ReportedFact {
            session_id: "session".into(),
            turn_id: None,
            fact_type: EventType::SessionReady,
            data: json!({"runtime_id":"runtime","client_session_key":"native"}),
        })
        .await
}

#[tokio::test]
async fn lifecycle_reuses_runtime_and_execution_facts_keep_it_running() {
    let (app, _root) = fixture().await;
    let created: String = sqlx::query_scalar("SELECT created_at FROM session_runtimes")
        .fetch_one(&app.db())
        .await
        .unwrap();
    assert_eq!(states(&app).await, ("starting".into(), "starting".into()));
    ready(&app).await.unwrap();
    assert_eq!(states(&app).await, ("idle".into(), "running".into()));
    let started = app
        .event_ingest_service()
        .report_fact(ReportedFact {
            session_id: "session".into(),
            turn_id: None,
            fact_type: EventType::TurnStarted,
            data: json!({"runtime_id":"runtime","input_summary":"work"}),
        })
        .await
        .unwrap();
    assert_eq!(states(&app).await, ("busy".into(), "running".into()));
    ready(&app).await.unwrap();
    assert_eq!(states(&app).await, ("busy".into(), "running".into()));
    app.event_ingest_service()
        .report_fact(ReportedFact {
            session_id: "session".into(),
            turn_id: started.turn_id,
            fact_type: EventType::TurnInterrupted,
            data: json!({"runtime_id":"runtime"}),
        })
        .await
        .unwrap();
    assert_eq!(states(&app).await, ("interrupted".into(), "running".into()));
    for _ in 0..2 {
        app.event_ingest_service()
            .report_fact(ReportedFact {
                session_id: "session".into(),
                turn_id: None,
                fact_type: EventType::SessionExited,
                data: json!({"runtime_id":"runtime","reason":"quit"}),
            })
            .await
            .unwrap();
    }
    assert_eq!(states(&app).await, ("exited".into(), "exited".into()));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM events WHERE event_type='session.exited'"
        )
        .fetch_one(&app.db())
        .await
        .unwrap(),
        1
    );
    app.event_ingest_service()
        .ingest_pontia_event(PontiaEvent::new(
            "session",
            None,
            PontiaEventSource::ExternalApi,
            "pi",
            PontiaEventType::SessionResuming,
            json!({"runtime_id":"runtime"}),
        ))
        .await
        .unwrap();
    assert_eq!(states(&app).await, ("starting".into(), "starting".into()));
    ready(&app).await.unwrap();
    assert_eq!(states(&app).await, ("idle".into(), "running".into()));
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT created_at FROM session_runtimes")
            .fetch_one(&app.db())
            .await
            .unwrap(),
        created
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT client_session_key FROM agent_bindings")
            .fetch_one(&app.db())
            .await
            .unwrap(),
        "native"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_runtimes")
            .fetch_one(&app.db())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn ready_rolls_back_both_projections_and_event_when_either_write_fails() {
    for table in ["sessions", "session_runtimes"] {
        let (app, _root) = fixture().await;
        sqlx::raw_sql(&format!("CREATE TRIGGER reject_state BEFORE UPDATE OF state ON {table} BEGIN SELECT RAISE(ABORT, 'injected projection failure'); END")).execute(&app.db()).await.unwrap();
        assert!(ready(&app).await.is_err());
        assert_eq!(states(&app).await, ("starting".into(), "starting".into()));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM events WHERE event_type='session.ready'"
            )
            .fetch_one(&app.db())
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agent_bindings")
                .fetch_one(&app.db())
                .await
                .unwrap(),
            0
        );
        sqlx::raw_sql("DROP TRIGGER reject_state")
            .execute(&app.db())
            .await
            .unwrap();
        ready(&app).await.unwrap();
        assert_eq!(states(&app).await, ("idle".into(), "running".into()));
    }
}

#[tokio::test]
async fn startup_failure_ends_runtime_and_preserves_reason() {
    let (app, _root) = fixture().await;
    app.event_ingest_service().ingest_pontia_event(PontiaEvent::new("session",None,PontiaEventSource::RuntimeManager,"pi",PontiaEventType::SessionError,json!({"runtime_id":"runtime","reason":"startup_failed","failure":{"message":"launcher failed"}}))).await.unwrap();
    assert_eq!(states(&app).await, ("error".into(), "exited".into()));
    let payload: String =
        sqlx::query_scalar("SELECT payload FROM events WHERE event_type='session.error'")
            .fetch_one(&app.db())
            .await
            .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&payload).unwrap()["reason"],
        "startup_failed"
    );
}

#[tokio::test]
async fn session_queries_return_one_session_with_all_runtimes() {
    let (app, root) = fixture().await;
    let workspace = pontia_application::upsert_workspace(&app.db(), root.path().to_str().unwrap())
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET workspace_id=? WHERE session_id='session'")
        .bind(workspace.workspace_id)
        .execute(&app.db())
        .await
        .unwrap();
    sqlx::query("INSERT INTO session_runtimes(runtime_id,session_id,role,state,created_at) VALUES ('another','session','tui','running','2000-01-01T00:00:00Z')").execute(&app.db()).await.unwrap();
    let sessions = app.queries().list_sessions(true, None, true).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].runtimes.len(), 2);
    assert_eq!(
        app.queries()
            .get_session("session")
            .await
            .unwrap()
            .unwrap()
            .runtimes
            .len(),
        2
    );
}

#[tokio::test]
async fn startup_timeout_uses_current_start_event_instead_of_runtime_creation() {
    let (app, _root) = fixture().await;
    sqlx::query("UPDATE session_runtimes SET created_at='2000-01-01T00:00:00Z'")
        .execute(&app.db())
        .await
        .unwrap();
    app.runtime_observer()
        .sweep_startup_timeouts()
        .await
        .unwrap();
    assert_eq!(states(&app).await, ("starting".into(), "starting".into()));
    sqlx::query(
        "UPDATE events SET created_at='2000-01-01T00:00:00Z' WHERE event_type='session.starting'",
    )
    .execute(&app.db())
    .await
    .unwrap();
    app.runtime_observer()
        .sweep_startup_timeouts()
        .await
        .unwrap();
    assert_eq!(states(&app).await, ("error".into(), "exited".into()));
}

#[tokio::test]
async fn old_process_observation_cannot_exit_restarted_runtime() {
    let (app, _root) = fixture().await;
    ready(&app).await.unwrap();
    app.event_ingest_service()
        .report_fact(ReportedFact {
            session_id: "session".into(),
            turn_id: None,
            fact_type: EventType::SessionExited,
            data: json!({"runtime_id":"runtime","reason":"quit"}),
        })
        .await
        .unwrap();
    app.event_ingest_service()
        .ingest_pontia_event(PontiaEvent::new(
            "session",
            None,
            PontiaEventSource::ExternalApi,
            "pi",
            PontiaEventType::SessionResuming,
            json!({"runtime_id":"runtime"}),
        ))
        .await
        .unwrap();
    assert!(app.event_ingest_service().ingest_runtime_observation_event(PontiaEvent::new("session",None,PontiaEventSource::RuntimeManager,"pi",PontiaEventType::SessionExited,json!({"runtime_id":"runtime","process_fingerprint":"old-process","reason":"process_exit"}))).await.is_err());
    assert_eq!(states(&app).await, ("starting".into(), "starting".into()));
}

#[tokio::test]
async fn restart_rejects_active_turn_and_terminal_session_without_changing_runtime() {
    let (app, root) = fixture().await;
    ready(&app).await.unwrap();
    let started = app
        .event_ingest_service()
        .report_fact(ReportedFact {
            session_id: "session".into(),
            turn_id: None,
            fact_type: EventType::TurnStarted,
            data: json!({"runtime_id":"runtime"}),
        })
        .await
        .unwrap();
    assert!(matches!(
        app.session_commands()
            .restart_session("session", root.path())
            .await,
        Err(pontia_core::Error::StateConflict(_))
    ));
    assert_eq!(states(&app).await, ("busy".into(), "running".into()));
    assert_eq!(
        app.event_ingest_service()
            .get_turn(&started.turn_id.unwrap())
            .await
            .unwrap()
            .unwrap()
            .state
            .to_string(),
        "running"
    );
    app.event_ingest_service()
        .report_fact(ReportedFact {
            session_id: "session".into(),
            turn_id: None,
            fact_type: EventType::SessionExited,
            data: json!({"runtime_id":"runtime","reason":"quit"}),
        })
        .await
        .unwrap();
    assert!(matches!(
        app.session_commands()
            .restart_session("session", root.path())
            .await,
        Err(pontia_core::Error::StateConflict(_))
    ));
    assert_eq!(states(&app).await, ("exited".into(), "exited".into()));
    let (app, root) = fixture().await;
    app.event_ingest_service().ingest_pontia_event(PontiaEvent::new("session",None,PontiaEventSource::RuntimeManager,"pi",PontiaEventType::SessionError,json!({"runtime_id":"runtime","reason":"startup_failed","failure":{"message":"launch failed"}}))).await.unwrap();
    assert!(matches!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            app.session_commands()
                .restart_session("session", root.path())
        )
        .await
        .unwrap(),
        Err(pontia_core::Error::StateConflict(_))
    ));
    assert_eq!(states(&app).await, ("error".into(), "exited".into()));
}
