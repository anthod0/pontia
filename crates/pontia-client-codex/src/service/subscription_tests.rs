use crate::runtime::CodexRuntime;
use futures_util::{SinkExt, StreamExt};
use pontia_application::{AppState, CreateSessionRequest, clients::ClientRegistry};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};
use tokio::{net::UnixListener, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{accept_async, tungstenite::Message};

struct Fixture {
    root: tempfile::TempDir,
    app: AppState,
    session: String,
    methods: Arc<Mutex<Vec<String>>>,
    unsubscribe_status: Arc<Mutex<String>>,
    rpc_errors: Arc<Mutex<HashMap<String, Value>>>,
    inputs: Arc<Mutex<Vec<Value>>>,
    turns: Arc<Mutex<Vec<Value>>>,
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
        let rpc_errors = Arc::new(Mutex::new(HashMap::<String, Value>::new()));
        let server_errors = rpc_errors.clone();
        let inputs = Arc::new(Mutex::new(Vec::new()));
        let server_inputs = inputs.clone();
        let turns = Arc::new(Mutex::new(Vec::new()));
        let server_turns = turns.clone();
        let codex_home = root.path().canonicalize().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut wire = accept_async(stream).await.unwrap();
            let mut materialized = false;
            while let Some(Ok(Message::Text(frame))) = wire.next().await {
                let request: Value = serde_json::from_str(&frame).unwrap();
                let Some(id) = request.get("id").cloned() else {
                    continue;
                };
                let method = request["method"].as_str().unwrap().to_owned();
                observed.lock().await.push(method.clone());
                let error = server_errors.lock().await.get(&method).cloned().or_else(|| {
                    if materialized {
                        return None;
                    }
                    match method.as_str() {
                        "thread/turns/list" => Some(json!({
                            "code":-32600,
                            "message":"thread thread-one is not materialized yet; thread/turns/list is unavailable before first user message"
                        })),
                        "thread/resume" => Some(json!({"code":-32600,"message":"no rollout found for thread id thread-one"})),
                        _ => None,
                    }
                });
                if let Some(error) = error {
                    wire.send(Message::Text(
                        json!({"id":id,"error":error}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                    continue;
                }
                let thread = json!({"id":"thread-one","cwd":codex_home,"canAcceptDirectInput":true,"status":{"type":"idle"}});
                let result = match method.as_str() {
                    "initialize" => {
                        json!({"userAgent":"codex/test","codexHome":codex_home,"platformFamily":"unix","platformOs":"linux"})
                    }
                    "thread/start" | "thread/resume" => {
                        json!({"thread":thread,"model":"model-one"})
                    }
                    "thread/read" => json!({"thread":thread}),
                    "thread/turns/list" => {
                        json!({"data":server_turns.lock().await.clone(),"nextCursor":null})
                    }
                    "turn/start" => {
                        materialized = true;
                        server_inputs.lock().await.push(request["params"].clone());
                        json!({"turn":{"id":"turn-one"}})
                    }
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
            rpc_errors,
            inputs,
            turns,
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
        self.app.inbox_commands().stop_scheduling().await;
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
    let inputs = fixture.inputs.lock().await;
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0]["threadId"], "thread-one");
    assert_eq!(inputs[0]["input"][0]["text"], "hello");
    drop(inputs);
    fixture.shutdown().await;
}

#[tokio::test]
async fn first_inbox_message_is_delivered_before_thread_history_exists() {
    let fixture = Fixture::new().await;
    let outcome = fixture
        .app
        .inbox_commands()
        .submit_message(
            &fixture.session,
            serde_json::from_value(json!({"input":"hi"})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.data["inbox_message"]["state"], "dispatched");
    let inputs = fixture.inputs.lock().await;
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0]["threadId"], "thread-one");
    assert_eq!(inputs[0]["input"][0]["text"], "hi");
    assert_eq!(
        inputs[0]["clientUserMessageId"],
        outcome.data["inbox_message"]["message_id"]
    );
    drop(inputs);
    fixture.shutdown().await;
}

#[tokio::test]
async fn failed_first_input_can_be_retried_on_the_same_unmaterialized_thread_after_resume() {
    let fixture = Fixture::new().await;
    fixture.rpc_errors.lock().await.insert(
        "turn/start".into(),
        json!({"code":-32600,"message":"input rejected"}),
    );
    let inbox = fixture.app.inbox_commands();
    let outcome = inbox
        .submit_message(
            &fixture.session,
            serde_json::from_value(json!({"input":"hi"})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.data["inbox_message"]["state"], "failed");
    let original = outcome.data["inbox_message"]["message_id"]
        .as_str()
        .unwrap();
    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    fixture.rpc_errors.lock().await.clear();

    let retried = inbox
        .retry_message(
            &fixture.app.session_commands(),
            &fixture.session,
            original,
            serde_json::from_value(json!({"message_id":"retry-first-input"})).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(retried.data["inbox_message"]["state"], "dispatched");
    let inputs = fixture.inputs.lock().await;
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0]["threadId"], "thread-one");
    assert_eq!(inputs[0]["input"][0]["text"], "hi");
    assert_eq!(inputs[0]["clientUserMessageId"], "retry-first-input");
    drop(inputs);
    assert_eq!(
        fixture
            .methods
            .lock()
            .await
            .iter()
            .filter(|method| method.as_str() == "thread/start")
            .count(),
        1
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn observer_recovers_a_created_session_with_a_bound_unmaterialized_thread_for_retry() {
    let fixture = Fixture::new().await;
    fixture.rpc_errors.lock().await.insert(
        "turn/start".into(),
        json!({"code":-32600,"message":"input rejected"}),
    );
    let inbox = fixture.app.inbox_commands();
    let outcome = inbox
        .submit_message(
            &fixture.session,
            serde_json::from_value(json!({"input":"hi"})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.data["inbox_message"]["state"], "failed");
    let original = outcome.data["inbox_message"]["message_id"]
        .as_str()
        .unwrap();
    // Reproduce the persisted Session/binding left by the original history-query failure.
    sqlx::query("UPDATE sessions SET state='created' WHERE session_id=?")
        .bind(&fixture.session)
        .execute(&fixture.app.db())
        .await
        .unwrap();
    let runtime = CodexRuntime::existing(fixture.root.path()).await.unwrap();
    runtime.clear_subscriptions().await;
    fixture.rpc_errors.lock().await.clear();
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let observer = tokio::spawn(
        super::CodexObserver::new(
            fixture.app.event_ingest_service(),
            fixture.root.path().into(),
        )
        .run(receiver),
    );

    inbox
        .retry_message(
            &fixture.app.session_commands(),
            &fixture.session,
            original,
            serde_json::from_value(json!({"message_id":"retry-created-input"})).unwrap(),
        )
        .await
        .unwrap();
    let delivered = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let message = inbox
                .get_message(&fixture.session, "retry-created-input")
                .await
                .unwrap()
                .unwrap();
            if !matches!(message.state.as_str(), "pending" | "dispatching") {
                break message;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(delivered.state, "dispatched");
    let inputs = fixture.inputs.lock().await;
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0]["threadId"], "thread-one");
    assert_eq!(inputs[0]["input"][0]["text"], "hi");
    drop(inputs);
    shutdown.send(true).unwrap();
    observer.await.unwrap();
    fixture.shutdown().await;
}

#[tokio::test]
async fn history_rejections_do_not_allow_input_on_an_existing_thread() {
    for error in [
        json!({"code":-32600,"message":"history is unavailable"}),
        json!({"code":-32600,"message":"thread different-thread is not materialized yet; thread/turns/list is unavailable before first user message"}),
        json!({"code":-32000,"message":"thread thread-one is not materialized yet; thread/turns/list is unavailable before first user message"}),
    ] {
        let fixture = Fixture::new().await;
        fixture.dispatch_first_input().await;
        fixture
            .rpc_errors
            .lock()
            .await
            .insert("thread/turns/list".into(), error);
        let error = fixture
            .app
            .turn_commands()
            .create_and_dispatch_turn(&fixture.session, "another input".into(), json!({}))
            .await
            .unwrap_err();
        assert!(matches!(error, pontia_core::Error::StateConflict(_)));
        assert_eq!(fixture.inputs.lock().await.len(), 1);
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn missing_rollout_does_not_restore_control_when_history_is_materialized() {
    let fixture = Fixture::new().await;
    fixture.dispatch_first_input().await;
    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    fixture.rpc_errors.lock().await.insert(
        "thread/resume".into(),
        json!({"code":-32600,"message":"no rollout found for thread id thread-one"}),
    );

    let error = fixture
        .app
        .session_commands()
        .resume_session(&fixture.session, fixture.root.path())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        pontia_core::Error::Conflict {
            code: "codex_thread_not_persisted",
            ..
        }
    ));
    assert_eq!(fixture.inputs.lock().await.len(), 1);
    fixture.shutdown().await;
}

#[tokio::test]
async fn active_native_turn_prevents_a_second_start_input() {
    let fixture = Fixture::new().await;
    fixture.dispatch_first_input().await;
    fixture
        .turns
        .lock()
        .await
        .push(json!({"id":"external-turn","status":"inProgress","items":[]}));

    let error = fixture
        .app
        .turn_commands()
        .create_and_dispatch_turn(&fixture.session, "another input".into(), json!({}))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        pontia_core::Error::Conflict {
            code: "input_busy",
            ..
        }
    ));
    assert_eq!(fixture.inputs.lock().await.len(), 1);
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
