use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_agent_control::{
    EventIngestService,
    client_contract::{ClientInteractions, ClientOperation, test_registration},
    clients::ClientRegistry,
    interactions::{
        ChoiceInteractionRequest, Interaction, InteractionInput, InteractionIntent,
        InteractionOption, InteractionRequest, InteractionResponse, InteractionState,
    },
};
use pontia_application::AppState;
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
        _: &'a InteractionResponse,
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
        pontia_agent_control::workspaces::upsert_canonical_workspace(
            &pool,
            root.path().canonicalize().unwrap().to_str().unwrap(),
            None,
        )
        .await
        .unwrap();
        pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
            pool.clone(),
        )
        .save("generic", "default", true)
        .await
        .unwrap();
        let resolver = Arc::new(Resolver {
            calls: AtomicUsize::new(0),
            entered: Notify::new(),
            release: Semaphore::new(0),
        });
        let mut registration = test_registration();
        registration.interactions = Some(resolver.clone());
        let mut clients = ClientRegistry::default();
        clients.register(registration);
        let app = AppState::builder(
            pontia_agent_control::AgentControl::builder(pool, root.path().into())
                .clients(clients)
                .build(),
        )
        .external_api_token(Some("interaction-test".into()))
        .build();
        let session = app
            .control()
            .session_commands()
            .create_session(
                serde_json::from_value(json!({"client_type":"generic","workspace":root.path()}))
                    .unwrap(),
            )
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
    fn choice(&self, key: &str) -> ChoiceInteractionRequest {
        ChoiceInteractionRequest {
            authorization_digest: "a".repeat(64),
            report_key: key.into(),
            blocked: None,
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
        }
    }
    async fn register(&self, key: &str) -> Interaction {
        self.app
            .control()
            .interactions()
            .register(self.choice(key))
            .await
            .unwrap()
    }
    async fn register_input(&self, key: &str, input: InteractionInput) -> Interaction {
        self.app
            .control()
            .interactions()
            .register(InteractionRequest {
                authorization_digest: "a".repeat(64),
                report_key: key.into(),
                blocked: None,
                session_id: self.session.clone(),
                turn_id: None,
                title: "Question".into(),
                description: None,
                input,
            })
            .await
            .unwrap()
    }
    fn path(&self, id: &str) -> String {
        format!("/api/v1/sessions/{}/interactions/{id}", self.session)
    }
}

#[tokio::test]
async fn extended_inputs_validate_and_store_type_specific_responses() {
    let f = Fixture::new().await;
    let options = f.choice("unused").options;
    let multi = f
        .register_input(
            "multi",
            InteractionInput::MultiSelect {
                options: options.clone(),
                allows_custom_text: true,
            },
        )
        .await;
    let multi_path = format!("{}/response", f.path(&multi.interaction_id));
    let (status, _) = request(
        f.app.clone(),
        "PUT",
        &multi_path,
        Some(json!({"option_keys":["proceed-once","proceed-once"]})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    f.resolver.release.add_permits(1);
    let (status, body) = request(
        f.app.clone(),
        "PUT",
        &multi_path,
        Some(json!({"option_keys":["proceed-once"],"custom_text":"Details"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"]["interaction"]["response"],
        json!({"option_keys":["proceed-once"],"custom_text":"Details"})
    );

    let text = f.register_input("text", InteractionInput::Text).await;
    let text_path = format!("{}/response", f.path(&text.interaction_id));
    let (status, _) = request(f.app.clone(), "PUT", &text_path, Some(json!({"text":"  "}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    f.resolver.release.add_permits(1);
    let (status, body) = request(
        f.app.clone(),
        "PUT",
        &text_path,
        Some(json!({"text":"Free-form answer"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"]["interaction"]["response"],
        json!({"text":"Free-form answer"})
    );

    let choice = f
        .register_input(
            "custom-choice",
            InteractionInput::Choice {
                options,
                allows_custom_text: true,
            },
        )
        .await;
    f.resolver.release.add_permits(1);
    let (status, body) = request(
        f.app.clone(),
        "PUT",
        &format!("{}/response", f.path(&choice.interaction_id)),
        Some(json!({"custom_text":"Another option"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"]["interaction"]["response"],
        json!({"custom_text":"Another option"})
    );
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
    let response = pontia_http::core_router(app)
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn first_choice_claims_once_and_completed_retries_return_saved_result() {
    let f = Fixture::new().await;
    let i = f.register("choice").await;
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
            .control()
            .session_commands()
            .create_session(
                serde_json::from_value(json!({"client_type":"generic","workspace":f.root.path()}))
                    .unwrap(),
            )
            .await
            .unwrap();
    }
    let request = Request::builder()
        .uri("/api/v1/dashboard/events/stream?after=session:0")
        .header("authorization", "Bearer interaction-test")
        .body(Body::empty())
        .unwrap();
    let response = pontia_http::core_router(f.app.clone())
        .oneshot(request)
        .await
        .unwrap();
    let mut stream = response.into_body();
    let interaction = f.register("choice").await;
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
    for key in ["one", "two", "three"] {
        ids.push(f.register(key).await.interaction_id);
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
async fn restart_preserves_confirmed_requests_and_expires_unconfirmed_requests_after_grace() {
    let f = Fixture::new().await;
    let pending = f.register("pending").await;
    let reconfirmed = f.register("reconfirmed").await;
    let resolving = f.register("resolving").await;
    sqlx::query("UPDATE interactions SET state='resolving' WHERE interaction_id=?")
        .bind(&resolving.interaction_id)
        .execute(&f.app.db())
        .await
        .unwrap();
    let resolved = f.register("choice").await;
    f.resolver.release.add_permits(1);
    f.app
        .control()
        .interactions()
        .respond(
            f.app.control(),
            &f.session,
            &resolved.interaction_id,
            "proceed-once",
        )
        .await
        .unwrap();
    let pool = pontia_storage_sqlite::connect_sqlite(&format!(
        "sqlite://{}",
        f.root.path().join("control.db").display()
    ))
    .await
    .unwrap();
    let restarted = AppState::builder(
        pontia_agent_control::AgentControl::builder(pool, f.root.path().into()).build(),
    )
    .external_api_token(Some("interaction-test".into()))
    .build();
    restarted
        .control()
        .interactions()
        .recover_pending()
        .await
        .unwrap();
    let service = restarted.control().interactions();
    let (status, body) = request(
        restarted.clone(),
        "PUT",
        &format!("{}/response", f.path(&pending.interaction_id)),
        Some(json!({"option_key":"proceed-once"})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "unavailable");
    assert_eq!(
        service
            .get(&f.session, &pending.interaction_id)
            .await
            .unwrap()
            .state,
        InteractionState::Pending
    );
    assert_eq!(
        service
            .register(f.choice("reconfirmed"))
            .await
            .unwrap()
            .interaction_id,
        reconfirmed.interaction_id
    );
    assert_eq!(
        service
            .get(&f.session, &resolving.interaction_id)
            .await
            .unwrap()
            .state,
        InteractionState::Expired
    );
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(60)).await;
    tokio::time::resume();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while service
            .get(&f.session, &pending.interaction_id)
            .await
            .unwrap()
            .state
            == InteractionState::Pending
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        service
            .get(&f.session, &reconfirmed.interaction_id)
            .await
            .unwrap()
            .state,
        InteractionState::Pending
    );
    for i in [pending, resolving] {
        assert_eq!(
            restarted
                .control()
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
        .control()
        .interactions()
        .get(&f.session, &resolved.interaction_id)
        .await
        .unwrap();
    assert_eq!(history.state, InteractionState::Resolved);
    assert_eq!(
        history.response.unwrap(),
        InteractionResponse::choice("proceed-once")
    );
}

#[tokio::test]
async fn lost_native_association_expires_without_recording_a_choice() {
    let f = Fixture::new().await;
    let interaction = f.register("choice").await;
    let disconnected = AppState::builder(
        pontia_agent_control::AgentControl::builder(f.app.db(), f.root.path().into()).build(),
    )
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
        .control()
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
    let response = pontia_http::core_router(f.app.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body();
    let i = f.register("choice").await;
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
        .control()
        .session_commands()
        .create_session(
            serde_json::from_value(json!({"client_type":"generic","workspace":f.root.path()}))
                .unwrap(),
        )
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
    let interaction = f.register("choice").await;
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
            .control()
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

#[tokio::test]
async fn concurrent_registrations_share_an_interaction_and_a_notification() {
    let f = Fixture::new().await;
    let service = f.app.control().interactions();
    let mut changes = service.subscribe();
    let request = f.choice("same-key");
    let (first, second) = tokio::join!(
        service.register(request.clone()),
        service.register(request.clone())
    );
    let first = first.unwrap();
    assert_eq!(second.unwrap(), first);
    assert_eq!(
        changes.recv().await.unwrap().interaction_id,
        first.interaction_id
    );
    let mut changed = request.clone();
    changed.description = Some("different".into());
    assert!(matches!(
        service.register(changed).await,
        Err(pontia_core::Error::StateConflict(_))
    ));
    assert!(changes.try_recv().is_err());
    service
        .expire(&f.session, &first.interaction_id)
        .await
        .unwrap();
    let replay = service.register(request).await.unwrap();
    assert_eq!(replay.interaction_id, first.interaction_id);
    assert_eq!(replay.state, InteractionState::Expired);
    assert_eq!(
        service
            .list(&f.session, None, 10, None)
            .await
            .unwrap()
            .interactions
            .len(),
        1
    );
}

#[tokio::test]
async fn settlement_replay_preserves_an_expired_background_interaction() {
    let f = Fixture::new().await;
    let service = f.app.control().interactions();
    let row = f.register("permission").await;
    service
        .expire(&f.session, &row.interaction_id)
        .await
        .unwrap();
    let digest = "a".repeat(64);
    for _ in 0..2 {
        assert_eq!(
            service
                .settle(&f.session, "permission", &digest, "settled", "resolved")
                .await
                .unwrap(),
            row.interaction_id
        );
    }
    assert!(matches!(
        service
            .settle(&f.session, "permission", &digest, "settled", "expired")
            .await,
        Err(pontia_core::Error::StateConflict(_))
    ));
    let saved = service.get(&f.session, &row.interaction_id).await.unwrap();
    assert_eq!(saved.state, InteractionState::Expired);
    assert!(saved.response.is_none());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM events WHERE session_id=? AND event_type='interaction.unblocked'",
    )
    .bind(&f.session)
    .fetch_one(&f.app.control().db())
    .await
    .unwrap();
    assert_eq!(count, 0);
}
