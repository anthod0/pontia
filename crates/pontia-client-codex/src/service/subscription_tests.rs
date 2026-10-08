use crate::runtime::CodexRuntime;
use futures_util::{SinkExt, StreamExt};
use pontia_application::{AppState, CreateSessionRequest, clients::ClientRegistry};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::{net::UnixListener, sync::Mutex};
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn session_uses_thread_subscription_without_creating_a_runtime() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("app-server.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let methods = Arc::new(Mutex::new(Vec::<String>::new()));
    let observed = methods.clone();
    let unsubscribe_status = Arc::new(Mutex::new("unsubscribed".to_string()));
    let server_unsubscribe_status = unsubscribe_status.clone();
    let codex_home = root.path().canonicalize().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut wire = accept_async(stream).await.unwrap();
        while let Some(Ok(Message::Text(frame))) = wire.next().await {
            let request: Value = serde_json::from_str(&frame).unwrap();
            let Some(id) = request.get("id").cloned() else {
                continue;
            };
            let method = request["method"].as_str().unwrap().to_owned();
            observed.lock().await.push(method.clone());
            let thread = json!({"id":"thread-one","cwd":codex_home,"canAcceptDirectInput":true,"status":{"type":"idle"}});
            let result = match method.as_str() {
                "initialize" => {
                    json!({"userAgent":"codex/test","codexHome":codex_home,"platformFamily":"unix","platformOs":"linux"})
                }
                "thread/start" | "thread/resume" | "thread/read" => {
                    json!({"thread":thread,"model":"model-one"})
                }
                "thread/turns/list" => json!({"data":[],"nextCursor":null}),
                "turn/start" => json!({"turn":{"id":"turn-one"}}),
                "model/list" => {
                    json!({"data":[{"model":"model-one","displayName":"Model One","description":"test","hidden":false}],"nextCursor":null})
                }
                "thread/settings/update" => json!({}),
                "thread/unsubscribe" => {
                    json!({"status":server_unsubscribe_status.lock().await.clone()})
                }
                other => panic!("unexpected method {other}"),
            };
            wire.send(Message::Text(
                json!({"id":id,"result":result}).to_string().into(),
            ))
            .await
            .unwrap();
        }
    });

    CodexRuntime::install_for_test(root.path(), socket)
        .await
        .unwrap();
    let pool = pontia_storage_sqlite::connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("test.db").display()
    ))
    .await
    .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(crate::registration());
    let app = AppState::builder(pool, root.path().into())
        .clients(clients)
        .build();
    let created = app
        .session_commands()
        .create_session(
            serde_json::from_value::<CreateSessionRequest>(
                json!({"client_type":"codex","workspace":root.path()}),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let session = created.session_id().unwrap().to_owned();
    sqlx::query("UPDATE sessions SET metadata=json_remove(metadata,'$.codex_control_root') WHERE session_id=?")
        .bind(&session)
        .execute(&app.db())
        .await
        .unwrap();
    super::CodexObserver::new(app.event_ingest_service(), root.path().into())
        .prepare()
        .await
        .unwrap();
    let control_root: String = sqlx::query_scalar(
        "SELECT json_extract(metadata,'$.codex_control_root') FROM sessions WHERE session_id=?",
    )
    .bind(&session)
    .fetch_one(&app.db())
    .await
    .unwrap();
    assert_eq!(
        control_root,
        root.path().canonicalize().unwrap().display().to_string()
    );

    app.turn_commands()
        .create_and_dispatch_turn(&session, "hello".into(), json!({}))
        .await
        .unwrap();
    let runtime_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_runtimes WHERE session_id=?")
            .bind(&session)
            .fetch_one(&app.db())
            .await
            .unwrap();
    assert_eq!(runtime_count, 0);
    let binding = pontia_application::AgentBindingService::new(app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.client_session_key, "thread-one");
    let models = app
        .session_commands()
        .list_session_models(&session)
        .await
        .unwrap();
    assert!(models.runtime_id.is_none());
    assert_eq!(models.models[0].id, "model-one");
    app.session_commands()
        .set_session_model(
            &session,
            pontia_application::sessions::SetSessionModelRequest {
                model: "model-one".into(),
                runtime_id: None,
            },
        )
        .await
        .unwrap();

    app.session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    assert_eq!(
        app.queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );
    app.session_commands()
        .resume_session(&session, root.path())
        .await
        .unwrap();
    assert_eq!(
        app.queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle"
    );
    assert_eq!(
        pontia_application::AgentBindingService::new(app.db())
            .binding_for_session(&session)
            .await
            .unwrap()
            .unwrap()
            .client_session_key,
        "thread-one"
    );

    *unsubscribe_status.lock().await = "accepted".into();
    let error = app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap_err();
    assert!(matches!(error, pontia_core::Error::ControlUnknown(_)));
    assert_eq!(
        app.queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle"
    );
    *unsubscribe_status.lock().await = "notSubscribed".into();
    app.session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    assert_eq!(
        app.queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );

    let methods = methods.lock().await.clone();
    assert!(methods.contains(&"thread/unsubscribe".into()));
    assert!(methods.contains(&"thread/resume".into()));
    assert!(
        !methods
            .iter()
            .any(|method| matches!(method.as_str(), "thread/archive" | "thread/unarchive"))
    );

    CodexRuntime::shutdown(root.path()).await;
    server.abort();
}
