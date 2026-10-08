use crate::runtime::CodexRuntime;
use futures_util::{SinkExt, StreamExt};
use pontia_application::{AppState, CreateSessionRequest, clients::ClientRegistry};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::{net::UnixListener, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{accept_async, tungstenite::Message};

struct Fixture {
    root: tempfile::TempDir,
    app: AppState,
    session: String,
    methods: Arc<Mutex<Vec<String>>>,
    unsubscribe_status: Arc<Mutex<String>>,
    server: JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
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
        Self {
            root,
            app,
            session,
            methods,
            unsubscribe_status,
            server,
        }
    }

    async fn dispatch_first_input(&self) {
        self.app
            .turn_commands()
            .create_and_dispatch_turn(&self.session, "hello".into(), json!({}))
            .await
            .unwrap();
    }

    async fn shutdown(self) {
        CodexRuntime::shutdown(self.root.path()).await;
        self.server.abort();
    }
}

#[tokio::test]
async fn first_input_restores_launch_data_and_binds_a_thread_without_a_runtime() {
    let fixture = Fixture::new().await;
    sqlx::query("UPDATE sessions SET metadata=json_remove(metadata,'$.codex_control_root','$.codex_launch_cwd') WHERE session_id=?")
        .bind(&fixture.session)
        .execute(&fixture.app.db())
        .await
        .unwrap();
    super::CodexObserver::new(
        fixture.app.event_ingest_service(),
        fixture.root.path().into(),
    )
    .prepare()
    .await
    .unwrap();
    let (control_root, launch_cwd): (String, String) = sqlx::query_as(
        "SELECT json_extract(metadata,'$.codex_control_root'),json_extract(metadata,'$.codex_launch_cwd') FROM sessions WHERE session_id=?",
    )
    .bind(&fixture.session)
    .fetch_one(&fixture.app.db())
    .await
    .unwrap();
    let root = fixture
        .root
        .path()
        .canonicalize()
        .unwrap()
        .display()
        .to_string();
    assert_eq!(control_root, root);
    assert_eq!(launch_cwd, root);

    fixture.dispatch_first_input().await;

    let runtime_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_runtimes WHERE session_id=?")
            .bind(&fixture.session)
            .fetch_one(&fixture.app.db())
            .await
            .unwrap();
    assert_eq!(runtime_count, 0);
    let binding = pontia_application::AgentBindingService::new(fixture.app.db())
        .binding_for_session(&fixture.session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.client_session_key, "thread-one");
    let methods = fixture.methods.lock().await;
    let start = methods
        .iter()
        .position(|method| method == "thread/start")
        .unwrap();
    let snapshot = methods
        .iter()
        .position(|method| method == "thread/turns/list")
        .unwrap();
    let input = methods
        .iter()
        .position(|method| method == "turn/start")
        .unwrap();
    assert!(start < snapshot && snapshot < input);
    drop(methods);
    fixture.shutdown().await;
}

#[tokio::test]
async fn model_control_does_not_require_a_runtime_identity() {
    let fixture = Fixture::new().await;
    fixture.dispatch_first_input().await;

    let models = fixture
        .app
        .session_commands()
        .list_session_models(&fixture.session)
        .await
        .unwrap();
    assert!(models.runtime_id.is_none());
    assert_eq!(models.models[0].id, "model-one");
    fixture
        .app
        .session_commands()
        .set_session_model(
            &fixture.session,
            pontia_application::sessions::SetSessionModelRequest {
                model: "model-one".into(),
                runtime_id: None,
            },
        )
        .await
        .unwrap();
    fixture.shutdown().await;
}

#[tokio::test]
async fn exit_and_resume_use_subscription_postconditions_without_archive_calls() {
    let fixture = Fixture::new().await;
    fixture.dispatch_first_input().await;

    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );
    fixture
        .app
        .session_commands()
        .resume_session(&fixture.session, fixture.root.path())
        .await
        .unwrap();
    assert_eq!(
        pontia_application::AgentBindingService::new(fixture.app.db())
            .binding_for_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .client_session_key,
        "thread-one"
    );

    *fixture.unsubscribe_status.lock().await = "accepted".into();
    let error = fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap_err();
    assert!(matches!(error, pontia_core::Error::ControlUnknown(_)));
    assert_eq!(
        fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle"
    );
    assert_eq!(
        fixture
            .methods
            .lock()
            .await
            .iter()
            .filter(|method| method.as_str() == "thread/unsubscribe")
            .count(),
        2,
        "an uncertain unsubscribe must not be replayed automatically"
    );

    *fixture.unsubscribe_status.lock().await = "notSubscribed".into();
    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    let methods = fixture.methods.lock().await;
    assert!(methods.contains(&"thread/resume".into()));
    assert!(
        !methods
            .iter()
            .any(|method| matches!(method.as_str(), "thread/archive" | "thread/unarchive"))
    );
    drop(methods);
    fixture.shutdown().await;
}
