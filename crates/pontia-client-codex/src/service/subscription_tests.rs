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
    before_thread_response: Arc<Mutex<Vec<Value>>>,
    rpc_errors: Arc<Mutex<HashMap<String, Value>>>,
    inputs: Arc<Mutex<Vec<Value>>>,
    turns: Arc<Mutex<Vec<Value>>>,
    configuration: Arc<Mutex<Value>>,
    thread_requests: Arc<Mutex<Vec<Value>>>,
    server: JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("app-server.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let methods = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed = methods.clone();
        let before_thread_response = Arc::new(Mutex::new(Vec::<Value>::new()));
        let server_before_thread_response = before_thread_response.clone();
        let rpc_errors = Arc::new(Mutex::new(HashMap::<String, Value>::new()));
        let server_errors = rpc_errors.clone();
        let inputs = Arc::new(Mutex::new(Vec::new()));
        let server_inputs = inputs.clone();
        let turns = Arc::new(Mutex::new(Vec::new()));
        let server_turns = turns.clone();
        let codex_home = root.path().canonicalize().unwrap();
        let configuration = Arc::new(Mutex::new(json!({})));
        let server_configuration = configuration.clone();
        let thread_requests = Arc::new(Mutex::new(Vec::new()));
        let server_thread_requests = thread_requests.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut wire = accept_async(stream).await.unwrap();
            let mut materialized = false;
            let mut unloaded = false;
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
                let thread = json!({"id":"thread-one","cwd":codex_home,"canAcceptDirectInput":true,"status":{"type":if unloaded { "notLoaded" } else { "idle" }}});
                let result = match method.as_str() {
                    "config/read" => {
                        assert_eq!(request["params"]["cwd"], json!(codex_home));
                        json!({"config":server_configuration.lock().await.clone()})
                    }
                    "initialize" => {
                        json!({"userAgent":"codex/test","codexHome":codex_home,"platformFamily":"unix","platformOs":"linux"})
                    }
                    "thread/start" | "thread/resume" => {
                        unloaded = false;
                        let mut thread = thread;
                        thread["status"]["type"] = json!("idle");
                        server_thread_requests.lock().await.push(request.clone());
                        for notification in server_before_thread_response.lock().await.drain(..) {
                            wire.send(Message::Text(notification.to_string().into()))
                                .await
                                .unwrap();
                        }
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
                    "thread/archive" => {
                        materialized = true;
                        unloaded = true;
                        json!({})
                    }
                    "thread/unarchive" => json!({"thread":thread}),
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
        sqlx::query("UPDATE execution_profiles SET system_prompt_template='creation-marker' WHERE profile_id='default' AND version='2'")
            .execute(&pool).await.unwrap();
        pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
            pool.clone(),
        )
        .save("codex", "default", Some(true), None)
        .await
        .unwrap();
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
            before_thread_response,
            rpc_errors,
            inputs,
            turns,
            configuration,
            thread_requests,
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
async fn start_and_resume_preserve_environment_filters_and_override_session_identity() {
    for (policy, expected_filter) in [
        (json!({}), json!({})),
        (json!({"include_only":[]}), json!({})),
        (json!({"exclude":["PROBE_DROP"]}), json!({})),
        (json!({"filters":{"PROBE_DROP":"exclude"}}), json!({})),
        (
            json!({"include_only":["PATH","PROBE_KEEP"]}),
            json!({"shell_environment_policy.include_only":["PATH","PROBE_KEEP","PONTIA_SESSION_ID"]}),
        ),
        (
            json!({"filters":{"PATH":"include","PROBE_KEEP":"include","PROBE_DROP":"exclude","pontia_session_id":"exclude"}}),
            json!({"shell_environment_policy.filters":{"PATH":"include","PROBE_KEEP":"include","PROBE_DROP":"exclude","PONTIA_SESSION_ID":"include"}}),
        ),
    ] {
        let fixture = Fixture::new().await;
        *fixture.configuration.lock().await = json!({"shell_environment_policy":policy});
        let mut environment = std::collections::BTreeMap::new();
        environment.insert(
            "PONTIA_SESSION_ID".into(),
            "wrong-configured-session".into(),
        );
        environment.insert("PROBE_KEEP".into(), "keep".into());
        super::CodexService::new(fixture.app.event_ingest_service())
            .provision(
                &fixture.session,
                fixture.root.path(),
                fixture.root.path(),
                &environment,
            )
            .await
            .unwrap();
        fixture.dispatch_first_input().await;
        fixture
            .app
            .session_commands()
            .terminate_session(&fixture.session)
            .await
            .unwrap();
        fixture
            .app
            .session_commands()
            .resume_session(&fixture.session, fixture.root.path())
            .await
            .unwrap();
        let requests = fixture.thread_requests.lock().await;
        assert_eq!(requests.len(), 2);
        let mut expected = expected_filter;
        expected["developer_instructions"] = json!("creation-marker");
        expected["shell_environment_policy.set"] =
            json!({"PROBE_KEEP":"keep","PONTIA_SESSION_ID":fixture.session});
        for request in requests.iter() {
            assert_eq!(request["params"]["config"], expected);
        }
        drop(requests);
        fixture.shutdown().await;
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
    super::CodexObserver::new(&fixture.app, fixture.root.path().into())
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
        super::CodexObserver::new(&fixture.app, fixture.root.path().into()).run(receiver),
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
async fn exit_unloads_and_resume_preserves_native_identity() {
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
    fixture.before_thread_response.lock().await.push(json!({
        "method":"thread/status/changed",
        "params":{"threadId":"thread-one","status":{"type":"notLoaded"}}
    }));
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

    let methods = fixture.methods.lock().await;
    assert!(
        !methods
            .iter()
            .any(|method| method == "turn/interrupt" || method == "thread/unsubscribe")
    );
    assert_eq!(
        methods
            .iter()
            .filter(|method| *method == "thread/archive")
            .count(),
        1
    );
    assert_eq!(
        methods
            .iter()
            .filter(|method| *method == "thread/unarchive")
            .count(),
        1
    );
    drop(methods);
    fixture.shutdown().await;
}

#[tokio::test]
async fn approval_received_before_binding_is_presented_after_first_input() {
    let fixture = Fixture::new().await;
    fixture.before_thread_response.lock().await.push(json!({
        "id": "early-approval", "method": "item/commandExecution/requestApproval",
        "params": {"threadId":"thread-one", "command":"echo approved"}
    }));
    let runtime = CodexRuntime::existing(fixture.root.path()).await.unwrap();
    let connection = runtime.connection().await.unwrap();
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let observer = tokio::spawn(
        super::CodexObserver::new(&fixture.app, fixture.root.path().into()).run(receiver),
    );
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while connection.events.receiver_count() == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    fixture.dispatch_first_input().await;
    let interactions = fixture.app.interactions();
    let approval = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let pending = interactions
                .list(
                    &fixture.session,
                    Some(pontia_application::interactions::InteractionState::Pending),
                    10,
                    None,
                )
                .await
                .unwrap();
            if let Some(interaction) = pending.interactions.into_iter().next() {
                break interaction;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(approval.session_id, fixture.session);
    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if interactions
                .list(
                    &fixture.session,
                    Some(pontia_application::interactions::InteractionState::Expired),
                    10,
                    None,
                )
                .await
                .unwrap()
                .interactions
                .len()
                == 1
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    shutdown.send(true).unwrap();
    observer.await.unwrap();
    fixture.shutdown().await;
}

#[tokio::test]
async fn creating_a_session_wakes_an_observer_with_no_active_sessions() {
    let mut fixture = Fixture::new().await;
    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let alias = fixture.root.path().join("state-alias");
    std::os::unix::fs::symlink(fixture.root.path(), &alias).unwrap();
    let observer = tokio::spawn(super::CodexObserver::new(&fixture.app, alias).run(receiver));
    fixture.session = fixture.app.session_commands().create_session(
        serde_json::from_value(json!({"client_type":"codex","workspace":fixture.root.path(),"initial_task":{"input":"hello"}})).unwrap()
    ).await.unwrap().session_id().unwrap().to_owned();
    let connection = CodexRuntime::existing(fixture.root.path())
        .await
        .unwrap()
        .connection()
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while connection.events.receiver_count() == 0 || fixture.inputs.lock().await.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fixture.inputs.lock().await[0]["input"][0]["text"], "hello");
    shutdown.send(true).unwrap();
    observer.await.unwrap();
    fixture.shutdown().await;
}

#[tokio::test]
async fn profile_survives_first_input_and_resume_without_mutable_profiles() {
    let fixture = Fixture::new().await;
    *fixture.configuration.lock().await = json!({"developer_instructions":"native-marker"});
    sqlx::query("DROP TABLE execution_profiles")
        .execute(&fixture.app.db())
        .await
        .unwrap();
    fixture.dispatch_first_input().await;
    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    fixture
        .app
        .session_commands()
        .resume_session(&fixture.session, fixture.root.path())
        .await
        .unwrap();
    let requests = fixture.thread_requests.lock().await;
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(
            request["params"]["config"]["developer_instructions"],
            "native-marker\n\ncreation-marker"
        );
    }
    drop(requests);
    fixture.shutdown().await;
}
