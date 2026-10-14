use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_application::{
    AppState, EventIngestService,
    client_contract::{ClientInteractions, ClientOperation, test_registration},
    clients::ClientRegistry,
    interactions::{
        ChoiceInteractionRequest, Interaction, InteractionIntent, InteractionOption,
        InteractionState,
    },
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{Notify, Semaphore};
use tower::ServiceExt;

struct Resolver {
    calls: AtomicUsize,
    entered: Notify,
    release: Semaphore,
}
impl ClientInteractions for Resolver {
    fn resolve<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.notify_one();
            self.release.acquire().await.unwrap().forget();
            Ok(())
        })
    }
}
struct Fixture {
    app: AppState,
    session: String,
    resolver: Arc<Resolver>,
    root: tempfile::TempDir,
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = pontia_storage_sqlite::connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("control.db").display()
        ))
        .await
        .unwrap();
        pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
        let resolver = Arc::new(Resolver {
            calls: AtomicUsize::new(0),
            entered: Notify::new(),
            release: Semaphore::new(0),
        });
        let mut registration = test_registration();
        registration.interactions = Some(resolver.clone());
        let mut clients = ClientRegistry::default();
        clients.register(registration);
        let app = AppState::builder(pool, root.path().into())
            .external_api_token(Some("interaction-test".into()))
            .clients(clients)
            .build();
        let session = app
            .session_commands()
            .create_session(serde_json::from_value(json!({"client_type":"generic"})).unwrap())
            .await
            .unwrap()
            .session_id()
            .unwrap()
            .to_owned();
        Self {
            app,
            session,
            resolver,
            root,
        }
    }
    async fn register(&self) -> Interaction {
        self.app
            .interactions()
            .register(ChoiceInteractionRequest {
                session_id: self.session.clone(),
                turn_id: None,
                title: "Choose".into(),
                description: None,
                options: vec![
                    InteractionOption {
                        key: "proceed-once".into(),
                        label: "Proceed".into(),
                        intent: InteractionIntent::Positive,
                    },
                    InteractionOption {
                        key: "reject-once".into(),
                        label: "Reject".into(),
                        intent: InteractionIntent::Negative,
                    },
                ],
            })
            .await
            .unwrap()
    }
    fn path(&self, id: &str) -> String {
        format!("/api/v1/sessions/{}/interactions/{id}", self.session)
    }
}
async fn request(
    app: AppState,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", "Bearer interaction-test")
        .body(
            body.map(|v| Body::from(v.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let response = pontia_http::router(app).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn first_choice_claims_once_and_completed_retries_return_saved_result() {
    let f = Fixture::new().await;
    let i = f.register().await;
    let path = format!("{}/response", f.path(&i.interaction_id));
    let (status, body) = request(
        f.app.clone(),
        "PUT",
        &path,
        Some(json!({"option_key":"unknown"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "invalid_interaction_option");
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
    let app = f.app.clone();
    let first_path = path.clone();
    let first = tokio::spawn(async move {
        request(
            app,
            "PUT",
            &first_path,
            Some(json!({"option_key":"proceed-once"})),
        )
        .await
    });
    f.resolver.entered.notified().await;
    let (status, body) = request(
        f.app.clone(),
        "PUT",
        &path,
        Some(json!({"option_key":"reject-once"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "interaction_resolution_in_progress");
    f.resolver.release.add_permits(1);
    let (status, first_body) = first.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first_body["data"]["interaction"]["state"], "resolved");
    assert_eq!(
        first_body["data"]["interaction"]["response"]["option_key"],
        "proceed-once"
    );
    assert!(first_body["data"]["interaction"]["resolved_at"].is_string());
    for option in ["reject-once", "not-even-an-option"] {
        let (status, body) = request(
            f.app.clone(),
            "PUT",
            &path,
            Some(json!({"option_key":option})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, first_body);
    }
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn transient_notifications_are_delivered_before_a_durable_backlog_is_drained() {
    let f = Fixture::new().await;
    for _ in 0..120 {
        f.app
            .session_commands()
            .create_session(serde_json::from_value(json!({"client_type":"generic"})).unwrap())
            .await
            .unwrap();
    }
    let request = Request::builder()
        .uri("/api/v1/dashboard/events/stream?after=session:0;task:0")
        .header("authorization", "Bearer interaction-test")
        .body(Body::empty())
        .unwrap();
    let response = pontia_http::router(f.app.clone())
        .oneshot(request)
        .await
        .unwrap();
    let mut stream = response.into_body();
    let interaction = f.register().await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let frame = stream.frame().await.unwrap().unwrap().into_data().unwrap();
            if std::str::from_utf8(&frame)
                .unwrap()
                .contains(&interaction.interaction_id)
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(3), stream.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    assert!(
        std::str::from_utf8(&frame)
            .unwrap()
            .contains("session_event")
    );
}

#[tokio::test]
async fn pages_are_ordered_scoped_and_validate_query_parameters() {
    let f = Fixture::new().await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(f.register().await.interaction_id);
    }
    let base = format!("/api/v1/sessions/{}/interactions", f.session);
    let (status, page) = request(
        f.app.clone(),
        "GET",
        &format!("{base}?state=pending&limit=2"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["data"]["interactions"][0]["interaction_id"], ids[0]);
    assert_eq!(page["data"]["interactions"][1]["interaction_id"], ids[1]);
    let cursor = page["data"]["next_cursor"].as_str().unwrap();
    let (_, next) = request(
        f.app.clone(),
        "GET",
        &format!("{base}?state=pending&limit=2&cursor={cursor}"),
        None,
    )
    .await;
    assert_eq!(next["data"]["interactions"].as_array().unwrap().len(), 1);
    assert_eq!(next["data"]["interactions"][0]["interaction_id"], ids[2]);
    assert!(next["data"]["next_cursor"].is_null());
    let (_, history) = request(f.app.clone(), "GET", &base, None).await;
    assert_eq!(history["data"]["interactions"][0]["interaction_id"], ids[2]);
    for query in [
        "state=bogus",
        "limit=0",
        "limit=201",
        "limit=1.5",
        "cursor=bad",
    ] {
        let (status, body) = request(f.app.clone(), "GET", &format!("{base}?{query}"), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_request");
    }
    let (status, _) = request(
        f.app.clone(),
        "GET",
        &format!("{base}?state=resolved&cursor={cursor}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for path in [
        "/api/v1/sessions/missing/interactions".to_owned(),
        format!("/api/v1/sessions/missing/interactions/{}", ids[0]),
        f.path("missing"),
    ] {
        let (status, body) = request(f.app.clone(), "GET", &path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "interaction_not_found");
    }
}

#[tokio::test]
async fn process_restart_expires_live_records_and_preserves_history() {
    let f = Fixture::new().await;
    let pending = f.register().await;
    let resolving = f.register().await;
    sqlx::query("UPDATE interactions SET state='resolving' WHERE interaction_id=?")
        .bind(&resolving.interaction_id)
        .execute(&f.app.db())
        .await
        .unwrap();
    let resolved = f.register().await;
    f.resolver.release.add_permits(1);
    f.app
        .interactions()
        .respond(&f.app, &f.session, &resolved.interaction_id, "proceed-once")
        .await
        .unwrap();
    let pool = pontia_storage_sqlite::connect_sqlite(&format!(
        "sqlite://{}",
        f.root.path().join("control.db").display()
    ))
    .await
    .unwrap();
    let restarted = AppState::builder(pool, f.root.path().into())
        .external_api_token(Some("interaction-test".into()))
        .build();
    restarted.interactions().expire_leftovers().await.unwrap();
    for i in [pending, resolving] {
        assert_eq!(
            restarted
                .interactions()
                .get(&f.session, &i.interaction_id)
                .await
                .unwrap()
                .state,
            InteractionState::Expired
        );
        let (status, body) = request(
            restarted.clone(),
            "PUT",
            &format!("{}/response", f.path(&i.interaction_id)),
            Some(json!({"option_key":"proceed-once"})),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "interaction_expired");
    }
    let history = restarted
        .interactions()
        .get(&f.session, &resolved.interaction_id)
        .await
        .unwrap();
    assert_eq!(history.state, InteractionState::Resolved);
    assert_eq!(history.response.unwrap().option_key, "proceed-once");
}

#[tokio::test]
async fn lost_native_association_expires_without_recording_a_choice() {
    let f = Fixture::new().await;
    let interaction = f.register().await;
    let disconnected = AppState::builder(f.app.db(), f.root.path().into())
        .external_api_token(Some("interaction-test".into()))
        .build();
    let path = format!("{}/response", f.path(&interaction.interaction_id));
    let (status, body) = request(
        disconnected.clone(),
        "PUT",
        &path,
        Some(json!({"option_key":"proceed-once"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "interaction_expired");
    let record = disconnected
        .interactions()
        .get(&f.session, &interaction.interaction_id)
        .await
        .unwrap();
    assert_eq!(record.state, InteractionState::Expired);
    assert!(record.response.is_none());
    assert!(record.resolved_at.is_none());
}

#[tokio::test]
async fn transient_interaction_sse_does_not_replace_the_durable_cursor() {
    let f = Fixture::new().await;
    let request = Request::builder()
        .uri("/api/v1/dashboard/events/stream")
        .header("authorization", "Bearer interaction-test")
        .body(Body::empty())
        .unwrap();
    let response = pontia_http::router(f.app.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body();
    let i = f.register().await;
    let frame = tokio::time::timeout(std::time::Duration::from_secs(3), stream.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    let frame = std::str::from_utf8(&frame).unwrap();
    assert!(frame.contains("interaction_changed"));
    assert!(frame.contains(&i.interaction_id));
    assert!(!frame.lines().any(|line| line.starts_with("id:")));
    f.app
        .session_commands()
        .create_session(serde_json::from_value(json!({"client_type":"generic"})).unwrap())
        .await
        .unwrap();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(3), stream.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    let frame = std::str::from_utf8(&frame).unwrap();
    assert!(frame.contains("session_event"));
    assert!(frame.lines().any(|line| line.starts_with("id: session:")));
}

#[tokio::test]
async fn completion_storage_failure_releases_the_claim_without_replaying_native_response() {
    let f = Fixture::new().await;
    let interaction = f.register().await;
    sqlx::query("CREATE TRIGGER fail_interaction_completion BEFORE UPDATE OF state ON interactions WHEN NEW.state='resolved' BEGIN SELECT RAISE(ABORT,'test completion failure'); END").execute(&f.app.db()).await.unwrap();
    f.resolver.release.add_permits(1);
    let path = format!("{}/response", f.path(&interaction.interaction_id));
    let (status, body) = request(
        f.app.clone(),
        "PUT",
        &path,
        Some(json!({"option_key":"proceed-once"})),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"]["code"], "internal_error");
    assert_eq!(
        f.app
            .interactions()
            .get(&f.session, &interaction.interaction_id)
            .await
            .unwrap()
            .state,
        InteractionState::Expired
    );
    let (status, _) = request(
        f.app.clone(),
        "PUT",
        &path,
        Some(json!({"option_key":"proceed-once"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 1);
}
