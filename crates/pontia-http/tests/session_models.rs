mod common;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, CreateSessionRequest, ReportedFact};
use pontia_core::domain::EventType;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn fixture() -> (tempfile::TempDir, AppState, String) {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("models.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    let state = AppState::builder(pool, root.path().into())
        .clients(crate::common::clients::clients())
        .external_api_token(Some("test-token".into()))
        .build();
    let request: CreateSessionRequest =
        serde_json::from_value(json!({"client_type":"codex","workspace":root.path()})).unwrap();
    let session = state
        .session_commands()
        .create_session(request)
        .await
        .unwrap()
        .session_id()
        .unwrap()
        .to_owned();
    (root, state, session)
}

async fn request(
    state: &AppState,
    session: &str,
    method: &str,
    resource: &str,
    body: Value,
    authenticated: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(format!("/api/v1/sessions/{session}{resource}"))
        .header("content-type", "application/json");
    if authenticated {
        request = request.header("authorization", "Bearer test-token");
    }
    let response = pontia_http::router(state.clone())
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (
        status,
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}

#[tokio::test]
async fn model_routes_enforce_authentication_capabilities_and_runtime_identity() {
    let (_root, state, session) = fixture().await;
    let change = json!({"model":"model-b","runtime_instance_id":"old"});
    for (method, path) in [("GET", "/models"), ("PATCH", "/model")] {
        assert_eq!(
            request(&state, &session, method, path, change.clone(), false)
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (_, snapshot) = request(&state, &session, "GET", "", Value::Null, true).await;
    assert_eq!(
        snapshot["data"]["session"]["capabilities"]["list_models"],
        true
    );
    assert_eq!(
        snapshot["data"]["session"]["capabilities"]["set_model"],
        true
    );
    assert!(snapshot["data"]["session"]["model_control_unavailable_reason"].is_string());
    assert_eq!(
        request(&state, &session, "GET", "/models", Value::Null, true)
            .await
            .0,
        StatusCode::CONFLICT
    );
    sqlx::query("UPDATE runtime_bindings SET runtime_instance_id='current',binding_state='confirmed',adapter_details=json_set(adapter_details,'$.codex.connection','available') WHERE session_id=?").bind(&session).execute(&state.db()).await.unwrap();
    state
        .event_ingest_service()
        .report_fact(ReportedFact {
            session_id: session.clone(),
            turn_id: None,
            fact_type: EventType::SessionReady,
            data: json!({"runtime_instance_id":"current","client_session_key":"thread"}),
        })
        .await
        .unwrap();
    let (status, _) = request(&state, &session, "PATCH", "/model", change.clone(), true).await;
    assert_eq!(status, StatusCode::CONFLICT);
    sqlx::query("UPDATE runtime_bindings SET capabilities=json_set(capabilities,'$.list_models',json('false'),'$.set_model',json('false')) WHERE session_id=?").bind(&session).execute(&state.db()).await.unwrap();
    for (method, path) in [("GET", "/models"), ("PATCH", "/model")] {
        let (status, body) = request(&state, &session, method, path, change.clone(), true).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "capability_unavailable");
    }
}
