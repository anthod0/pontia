mod common;
use axum::http::StatusCode;
use pontia_application::{
    AgentBindingService, AppState, CreateSessionRequest, SessionCommandService,
    UpsertAgentBindingRequest,
};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};

async fn app() -> (tempfile::TempDir, AppState, String) {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("db.sqlite").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    let request: CreateSessionRequest =
        serde_json::from_value(json!({"client_type":"codex","workspace":root.path()})).unwrap();
    let created = SessionCommandService::new(
        pontia_application::EventIngestService::new(pool.clone())
            .with_clients(crate::common::clients::clients()),
        root.path().into(),
    )
    .create_session(request)
    .await
    .unwrap();
    let session = created.session_id().unwrap().to_string();
    sqlx::query("UPDATE runtime_bindings SET runtime_instance_id='instance',binding_state='confirmed' WHERE session_id=?").bind(&session).execute(&pool).await.unwrap();
    AgentBindingService::new(pool.clone())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session.clone(),
            client_type: "codex".into(),
            launch_cwd: root.path().display().to_string(),
            client_session_key: "thread".into(),
            client_session_file: None,
            metadata: json!({}),
        })
        .await
        .unwrap();
    let state = AppState::builder(pool, root.path().into())
        .clients(crate::common::clients::clients())
        .build();
    (root, state, session)
}

async fn report(state: &AppState, session: &str, kind: &str, data: Value) -> (StatusCode, Value) {
    crate::common::reporting::report_fact(
        state.clone(),
        json!({"session_id":session,"type":kind,"data":data}),
    )
    .await
}

#[tokio::test]
async fn native_turns_are_deduplicated_and_old_instances_cannot_report() {
    let (_root, state, session) = app().await;
    let data = json!({"runtime_instance_id":"instance","native_turn_id":"native-one","input":{"summary":"manual input"}});
    let ((status, first), (second_status, second)) = tokio::join!(
        report(&state, &session, "turn.started", data.clone()),
        report(&state, &session, "turn.started", data)
    );
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(second_status, StatusCode::OK, "{second}");
    assert_eq!(first["turn_id"], second["turn_id"]);
    assert!(first["duplicate"] == true || second["duplicate"] == true);
    let (status, _) = report(
        &state,
        &session,
        "turn.completed",
        json!({"runtime_instance_id":"old","native_turn_id":"native-one"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status,end)=report(&state,&session,"turn.interrupted",json!({"runtime_instance_id":"instance","native_turn_id":"native-one","native_completed_at":null,"observation":"snapshot"})).await;
    assert_eq!(status, StatusCode::OK, "{end}");
    let turns = pontia_application::ExternalQueryService::new(state.db())
        .list_turns(&session)
        .await
        .unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].state, "interrupted");
    assert_eq!(turns[0].completed_at, None);
}

#[tokio::test]
async fn archive_does_not_invent_a_turn_terminal_and_late_native_terminal_converges() {
    let (_root, state, session) = app().await;
    assert_eq!(
        report(
            &state,
            &session,
            "turn.started",
            json!({"runtime_instance_id":"instance","native_turn_id":"native-one"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        report(
            &state,
            &session,
            "session.exited",
            json!({"runtime_instance_id":"instance","reason":"thread_archived"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let query = pontia_application::ExternalQueryService::new(state.db());
    assert_eq!(
        query.list_turns(&session).await.unwrap()[0].state,
        "running"
    );
    assert_eq!(report(&state,&session,"turn.interrupted",json!({"runtime_instance_id":"instance","native_turn_id":"native-one","native_completed_at":null})).await.0,StatusCode::OK);
    assert_eq!(
        query.list_turns(&session).await.unwrap()[0].state,
        "interrupted"
    );
    assert_eq!(
        query.get_session(&session).await.unwrap().unwrap().state,
        "exited"
    );
}
