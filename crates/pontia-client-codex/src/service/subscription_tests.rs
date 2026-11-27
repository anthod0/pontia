use crate::runtime::CodexRuntime;
use futures_util::{SinkExt, StreamExt};
use pontia_agent_control::{AgentControl, CreateSessionRequest, clients::ClientRegistry};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};
use tokio::{net::UnixListener, sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{accept_async, tungstenite::Message};

struct Fixture {
    root: tempfile::TempDir,
    app: AgentControl,
    session: String,
    methods: Arc<Mutex<Vec<String>>>,
    before_thread_response: Arc<Mutex<Vec<Value>>>,
    rpc_errors: Arc<Mutex<HashMap<String, Value>>>,
    inputs: Arc<Mutex<Vec<Value>>>,
    interrupts: Arc<Mutex<Vec<Value>>>,
    turns: Arc<Mutex<Vec<Value>>>,
    configuration: Arc<Mutex<Value>>,
    thread_requests: Arc<Mutex<Vec<Value>>>,
    settings_requests: Arc<Mutex<Vec<Value>>>,
    thread_name: Arc<Mutex<Value>>,
    notifications: tokio::sync::mpsc::UnboundedSender<Value>,
    server: JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        Self::with_instructions(None).await
    }

    async fn with_instructions(system_instructions: Option<&str>) -> Self {
        Self::create(system_instructions, "default", None).await
    }

    async fn create(system_instructions: Option<&str>, mode: &str, effort: Option<&str>) -> Self {
        Self::create_with_request(system_instructions, mode, effort, |_| {}).await
    }

    async fn create_with_request(
        system_instructions: Option<&str>,
        mode: &str,
        effort: Option<&str>,
        configure: impl FnOnce(&mut CreateSessionRequest),
    ) -> Self {
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
        let interrupts = Arc::new(Mutex::new(Vec::new()));
        let server_interrupts = interrupts.clone();
        let turns = Arc::new(Mutex::new(Vec::new()));
        let server_turns = turns.clone();
        let codex_home = root.path().canonicalize().unwrap();
        let configuration = Arc::new(Mutex::new(json!({"mcp_servers":{}})));
        let server_configuration = configuration.clone();
        let thread_requests = Arc::new(Mutex::new(Vec::new()));
        let server_thread_requests = thread_requests.clone();
        let settings_requests = Arc::new(Mutex::new(Vec::new()));
        let server_settings_requests = settings_requests.clone();
        let thread_name = Arc::new(Mutex::new(Value::Null));
        let server_thread_name = thread_name.clone();
        let (notifications, mut notification_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut wire = accept_async(stream).await.unwrap();
            let mut materialized = false;
            let mut unloaded = false;
            loop {
                let frame = tokio::select! {
                    frame = wire.next() => match frame {
                        Some(Ok(Message::Text(frame))) => frame,
                        _ => break,
                    },
                    Some(notification) = notification_rx.recv() => {
                        wire.send(Message::Text(notification.to_string().into())).await.unwrap();
                        continue;
                    }
                };
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
                let thread = json!({"id":"thread-one","name":server_thread_name.lock().await.clone(),"cwd":codex_home,"canAcceptDirectInput":true,"status":{"type":if unloaded { "notLoaded" } else { "idle" }}});
                let result = match method.as_str() {
                    "config/read" => {
                        assert_eq!(request["params"]["cwd"], json!(codex_home));
                        let mut reply = json!({"config":server_configuration.lock().await.clone()});
                        if request["params"]["includeLayers"] == true {
                            reply["layers"] = json!([{"name":{"type":"project","dotCodexFolder":codex_home.join(".codex")},
                            "disabledReason":"Project is untrusted","config":{"mcp_servers":{
                                "project.local":{"command":"local","args":["serve"]},
                                "project.remote":{"url":"https://example.test/mcp"}
                            }}}]);
                        }
                        reply
                    }
                    "skills/list" => {
                        assert_eq!(request["params"]["cwds"], json!([codex_home]));
                        json!({"data":[{"cwd":codex_home,"skills":[{"name":"example"}],"errors":[]}]})
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
                        json!({"thread":thread,"model":"model-one","reasoningEffort":if method == "thread/start" { "medium" } else { "low" }})
                    }
                    "thread/read" => json!({"thread":thread}),
                    "thread/turns/list" => {
                        json!({"data":server_turns.lock().await.clone(),"nextCursor":null})
                    }
                    "turn/start" | "turn/steer" => {
                        materialized = true;
                        server_inputs.lock().await.push(request["params"].clone());
                        json!({"turn":{"id":"turn-one"}})
                    }
                    "turn/interrupt" => {
                        server_interrupts
                            .lock()
                            .await
                            .push(request["params"].clone());
                        json!({})
                    }
                    "thread/settings/update" => {
                        server_settings_requests.lock().await.push(request.clone());
                        json!({})
                    }
                    "model/list" => {
                        json!({"data":[{"model":"model-one","displayName":"Model One","description":"test","hidden":false,"isDefault":true,"supportedReasoningEfforts":[]}],"nextCursor":null})
                    }
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
        .save("codex", "default", true)
        .await
        .unwrap();
        let mut clients = ClientRegistry::default();
        clients.register(crate::registration());
        let app = AgentControl::builder(pool, root.path().into())
            .clients(clients)
            .build();
        let mut request = serde_json::from_value(json!({
            "client_type":"codex",
            "workspace":root.path(),
            "system_instructions":system_instructions,
            "include_standalone_session_instructions":true,
            "mode":mode,
            "reasoning_effort":effort
        }))
        .unwrap();
        configure(&mut request);
        let created = app
            .session_commands()
            .create_session(request)
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
            interrupts,
            turns,
            configuration,
            thread_requests,
            settings_requests,
            thread_name,
            notifications,
            server,
        }
    }

    async fn dispatch_first_input(&self) {
        self.app
            .turn_commands()
            .start_turn(&self.session, "hello".into(), json!({}))
            .await
            .unwrap();
    }

    async fn delivery(
        &self,
    ) -> (
        pontia_agent_control::runtime::ControlTarget,
        Arc<dyn pontia_agent_control::client_contract::ClientSession>,
    ) {
        let target = pontia_agent_control::runtime::ControlTarget::resolve(
            &self.app.db(),
            &self.app.clients(),
            &self.session,
            None,
        )
        .await
        .unwrap();
        let client = self
            .app
            .clients()
            .get("codex")
            .unwrap()
            .session
            .clone()
            .unwrap();
        (target, client)
    }

    async fn start_observer(&self) -> (tokio::sync::watch::Sender<bool>, JoinHandle<()>) {
        self.methods.lock().await.clear();
        let (shutdown, receiver) = tokio::sync::watch::channel(false);
        let observer = tokio::spawn(
            super::CodexObserver::new(&self.app, self.root.path().into()).run(receiver),
        );
        // Wait for the observer's initial snapshot before sending a live notification.
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if self
                    .methods
                    .lock()
                    .await
                    .iter()
                    .any(|method| method == "thread/read")
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        (shutdown, observer)
    }

    async fn shutdown(self) {
        self.app.inbox_commands().stop_scheduling().await;
        CodexRuntime::shutdown(self.root.path()).await;
        self.server.abort();
    }
}

#[tokio::test]
async fn reasoning_effort_applies_only_at_launch_and_tracks_native_settings() {
    use pontia_core::domain::EventType;
    let fixture = Fixture::create(None, "default", Some("high")).await;
    let observed = || async {
        fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .reasoning_effort
    };
    fixture.dispatch_first_input().await;
    assert_eq!(
        fixture.thread_requests.lock().await[0]["params"]["config"]["model_reasoning_effort"],
        "high"
    );
    assert_eq!(observed().await.as_deref(), Some("medium"));

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
    assert_eq!(requests[1]["method"], "thread/resume");
    assert!(
        requests[1]["params"]["config"]
            .get("model_reasoning_effort")
            .is_none()
    );
    drop(requests);
    assert_eq!(observed().await.as_deref(), Some("low"));

    let (shutdown, observer) = fixture.start_observer().await;
    for effort in [Some("adaptive"), None] {
        let mut events = fixture.app.agent_events().subscribe();
        fixture.notifications.send(json!({
            "method":"thread/settings/updated",
            "params":{"threadId":"thread-one","threadSettings":{"model":"model-one","effort":effort}}
        })).unwrap();
        wait_for(&mut events, EventType::SessionReasoningEffortUpdated, None).await;
        assert_eq!(observed().await.as_deref(), effort);
    }
    shutdown.send(true).unwrap();
    observer.await.unwrap();
    fixture.shutdown().await;
}

#[tokio::test]
async fn running_sessions_apply_only_native_confirmed_reasoning_effort() {
    use pontia_core::domain::EventType;

    let fixture = Fixture::create(None, "default", Some("medium")).await;
    assert!(
        !fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .reasoning_effort_control
    );
    fixture.dispatch_first_input().await;
    assert!(
        fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .reasoning_effort_control
    );
    let runtime = CodexRuntime::for_session(&fixture.app.db(), &fixture.session)
        .await
        .unwrap()
        .unwrap();
    runtime
        .connection()
        .await
        .unwrap()
        .unloaded_threads
        .lock()
        .await
        .insert("thread-one".into(), 1);
    assert!(
        !fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .reasoning_effort_control
    );
    runtime
        .connection()
        .await
        .unwrap()
        .unloaded_threads
        .lock()
        .await
        .remove("thread-one");
    let (shutdown, observer) = fixture.start_observer().await;
    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='session.reasoning_effort_updated'",
    )
    .bind(&fixture.session)
    .fetch_one(&fixture.app.db())
    .await
    .unwrap();

    fixture
        .app
        .session_commands()
        .set_reasoning_effort(
            &fixture.session,
            pontia_agent_control::SetSessionReasoningEffortRequest {
                reasoning_effort: "high".into(),
            },
        )
        .await
        .unwrap();
    let request = fixture.settings_requests.lock().await[0].clone();
    assert_eq!(request["method"], "thread/settings/update");
    assert_eq!(
        request["params"],
        json!({"threadId":"thread-one","effort":"high"})
    );
    let unconfirmed = fixture
        .app
        .queries()
        .get_session(&fixture.session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unconfirmed.reasoning_effort.as_deref(), Some("medium"));
    let after_request: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='session.reasoning_effort_updated'",
    )
    .bind(&fixture.session)
    .fetch_one(&fixture.app.db())
    .await
    .unwrap();
    assert_eq!(after_request, before);

    let mut events = fixture.app.agent_events().subscribe();
    fixture
        .notifications
        .send(json!({
            "method":"thread/settings/updated",
            "params":{"threadId":"thread-one","threadSettings":{"model":"model-one","effort":"low"}}
        }))
        .unwrap();
    wait_for(&mut events, EventType::SessionReasoningEffortUpdated, None).await;
    let confirmed = fixture
        .app
        .queries()
        .get_session(&fixture.session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(confirmed.reasoning_effort.as_deref(), Some("low"));

    shutdown.send(true).unwrap();
    observer.await.unwrap();
    fixture.shutdown().await;
}

#[tokio::test]
async fn restricted_session_disables_native_agents_and_all_configured_mcp_servers() {
    let fixture = Fixture::create(None, "restricted", None).await;
    *fixture.configuration.lock().await = json!({"mcp_servers":{
        "alpha":{"command":"alpha"},
        "team.beta":{"url":"https://example.test/mcp"}
    }});
    fixture.dispatch_first_input().await;
    {
        let requests = fixture.thread_requests.lock().await;
        let config = &requests[0]["params"]["config"];
        assert_eq!(config["agents.enabled"], false);
        assert_eq!(config["features.multi_agent"], false);
        assert_eq!(config["features.multi_agent_v2"], false);
        assert_eq!(config["features.skip_host_skill_discovery"], true);
        assert_eq!(config["features.skill_search"], false);
        assert_eq!(config["features.apps"], false);
        assert_eq!(config["features.plugins"], false);
        assert_eq!(config["web_search"], "disabled");
        assert_eq!(
            config["tools.experimental_request_user_input.enabled"],
            false
        );
        assert_eq!(config["approval_policy"], "never");
        assert_eq!(config["sandbox_mode"], "danger-full-access");
        assert_eq!(
            config["mcp_servers"],
            json!({"alpha":{"enabled":false},"team.beta":{"enabled":false},
                "project.local":{"command":"local","enabled":false},
                "project.remote":{"url":"https://example.test/mcp","enabled":false}})
        );
        assert!(
            config
                .as_object()
                .unwrap()
                .keys()
                .all(|key| !key.starts_with("hooks"))
        );
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn restricted_session_fails_before_start_if_mcp_servers_cannot_be_enumerated() {
    for configuration in [None, Some(json!({})), Some(json!({"mcp_servers":[]}))] {
        let fixture = Fixture::create(None, "restricted", None).await;
        if let Some(configuration) = configuration {
            *fixture.configuration.lock().await = configuration;
        } else {
            fixture.rpc_errors.lock().await.insert(
                "config/read".into(),
                json!({"code":-32603,"message":"config read failed"}),
            );
        }
        let error = fixture
            .app
            .turn_commands()
            .start_turn(&fixture.session, "hello".into(), json!({}))
            .await
            .unwrap_err();
        assert!(error.to_string().to_lowercase().contains("mcp"), "{error}");
        assert!(fixture.thread_requests.lock().await.is_empty());
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn yolo_is_scoped_to_the_session_and_preserved_on_resume() {
    for yolo in [false, true] {
        let fixture = Fixture::create(None, if yolo { "yolo" } else { "default" }, None).await;
        *fixture.configuration.lock().await = json!({
            "approval_policy":"on-request",
            "sandbox_mode":"workspace-write",
            "mcp_servers":{"alpha":{"command":"alpha"}}
        });
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
        {
            let requests = fixture.thread_requests.lock().await;
            assert_eq!(requests.len(), 2);
            for request in requests.iter() {
                let config = &request["params"]["config"];
                assert!(config.get("mcp_servers").is_none());
                assert_eq!(
                    config["approval_policy"],
                    if yolo { json!("never") } else { Value::Null }
                );
                assert_eq!(
                    config["sandbox_mode"],
                    if yolo {
                        json!("danger-full-access")
                    } else {
                        Value::Null
                    }
                );
            }
        }
        assert_eq!(
            fixture.configuration.lock().await["sandbox_mode"],
            "workspace-write"
        );
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn discovered_skills_are_disabled_only_for_the_restricted_session_across_resume() {
    for disable_skills in [false, true] {
        let fixture = Fixture::create(
            None,
            if disable_skills {
                "restricted"
            } else {
                "default"
            },
            None,
        )
        .await;
        *fixture.configuration.lock().await = json!({"skills":{"include_instructions":true,"config":[
                {"path":"/previously-disabled/SKILL.md","enabled":false},
                {"name":"example","enabled":true}
            ]},"mcp_servers":{}});
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
        {
            let requests = fixture.thread_requests.lock().await;
            assert_eq!(requests.len(), 2);
            for request in requests.iter() {
                assert_eq!(
                    request["params"]["config"]["skills.config"],
                    if disable_skills {
                        json!([
                            {"path":"/previously-disabled/SKILL.md","enabled":false},
                            {"name":"example","enabled":false}
                        ])
                    } else {
                        Value::Null
                    }
                );
                assert_eq!(
                    request["params"]["config"]["skills.include_instructions"],
                    if disable_skills {
                        json!(false)
                    } else {
                        Value::Null
                    }
                );
            }
        }
        assert_eq!(
            fixture.configuration.lock().await["skills"]["include_instructions"],
            true
        );
        fixture.shutdown().await;
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
        let fixture = Fixture::create_with_request(None, "default", None, |request| {
            request.model = Some("model-two".into());
            request.credential_env = Some("PONTIA_BOT_TOOL_TOKEN".into());
            request.runtime_environment = [
                (
                    "PONTIA_SESSION_ID".into(),
                    "wrong-configured-session".into(),
                ),
                ("PROBE_KEEP".into(), "keep".into()),
            ]
            .into();
        })
        .await;
        *fixture.configuration.lock().await = json!({"shell_environment_policy":policy});
        let metadata: String =
            sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id=?")
                .bind(&fixture.session)
                .fetch_one(&fixture.app.db())
                .await
                .unwrap();
        assert!(!metadata.contains("codex_environment"));
        assert!(!metadata.contains("fixture-token"));
        fixture.dispatch_first_input().await;
        assert_eq!(
            fixture.thread_requests.lock().await[0]["params"]["model"],
            "model-two"
        );
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
        expected["developer_instructions"] =
            json!(pontia_agent_control::standalone_session_instructions());
        let token = requests[0]["params"]["config"]["shell_environment_policy.set"]["PONTIA_BOT_TOOL_TOKEN"].as_str().unwrap();
        assert!(
            fixture
                .app
                .queries()
                .verify_session_credential(&fixture.session, "PONTIA_BOT_TOOL_TOKEN", token)
                .await
                .unwrap()
        );
        let environment: String =
            sqlx::query_scalar("SELECT runtime_environment FROM sessions WHERE session_id=?")
                .bind(&fixture.session)
                .fetch_one(&fixture.app.db())
                .await
                .unwrap();
        assert!(!environment.contains(token));
        assert!(!environment.contains("PONTIA_BOT_TOOL_TOKEN"));
        expected["shell_environment_policy.set"] = json!({"PROBE_KEEP":"keep","PONTIA_SESSION_ID":fixture.session,"PONTIA_BOT_TOOL_TOKEN":token});
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
    let binding = pontia_agent_control::AgentBindingService::new(fixture.app.db())
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
    drop(inputs);
    fixture.shutdown().await;
}

#[tokio::test]
async fn native_user_message_does_not_recreate_delivered_queue_input() {
    let fixture = Fixture::new().await;
    fixture
        .app
        .inbox_commands()
        .submit_message(
            &fixture.session,
            serde_json::from_value(json!({"input":"hi"})).unwrap(),
        )
        .await
        .unwrap();
    let service = super::CodexService::new(fixture.app.event_ingest_service());
    let native = json!({"id":"native-turn","status":"completed","items":[{
        "id":"user-item","type":"userMessage","content":[{"type":"text","text":"hi"}]
    }]});
    service
        .turn_fact(&fixture.session, &native, "snapshot")
        .await
        .unwrap();
    let snapshot = fixture
        .app
        .queries()
        .session_snapshot(&fixture.session)
        .await
        .unwrap()
        .unwrap();
    assert!(!snapshot.has_unsettled_input);
    assert_eq!(snapshot.turns[0].state, "completed");
    assert_eq!(fixture.inputs.lock().await.len(), 1);
    fixture.shutdown().await;
}

#[tokio::test]
async fn rejected_thread_start_allows_the_unbound_session_to_exit() {
    let fixture = Fixture::new().await;
    fixture.rpc_errors.lock().await.insert(
        "thread/start".into(),
        json!({"code":-32600,"message":"startup rejected"}),
    );
    let outcome = fixture
        .app
        .inbox_commands()
        .submit_message(
            &fixture.session,
            serde_json::from_value(json!({"input":"hi"})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.data["inbox_message"]["state"], "failed");
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
            .start_turn(&fixture.session, "another input".into(), json!({}))
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
        .start_turn(&fixture.session, "another input".into(), json!({}))
        .await
        .unwrap_err();
    assert!(matches!(error, pontia_core::Error::StateConflict(_)));
    assert_eq!(fixture.inputs.lock().await.len(), 1);
    fixture.shutdown().await;
}

#[tokio::test]
async fn native_turn_state_changes_report_state_changed() {
    let fixture = Fixture::new().await;
    fixture.dispatch_first_input().await;
    fixture
        .turns
        .lock()
        .await
        .push(json!({"id":"external-turn","status":"inProgress","items":[]}));
    fixture.rpc_errors.lock().await.insert(
        "turn/steer".into(),
        json!({"code":-32000,"message":"Turn is no longer active"}),
    );
    let (target, client) = fixture.delivery().await;

    let start = client
        .start_turn(
            fixture.app.event_ingest_service(),
            &target,
            &pontia_agent_control::InputPayload::from("hello"),
            None,
        )
        .await;
    let steer = client
        .steer(
            fixture.app.event_ingest_service(),
            &target,
            &pontia_agent_control::InputPayload::from("follow up"),
        )
        .await;

    assert!(matches!(
        start,
        Err(pontia_agent_control::DeliveryError::StateChanged(_))
    ));
    assert!(matches!(
        steer,
        Err(pontia_agent_control::DeliveryError::StateChanged(_))
    ));
    fixture.shutdown().await;
}

#[tokio::test]
async fn missing_native_active_turn_reports_state_changed() {
    let fixture = Fixture::new().await;
    fixture.dispatch_first_input().await;
    let (target, client) = fixture.delivery().await;

    let steer = client
        .steer(
            fixture.app.event_ingest_service(),
            &target,
            &pontia_agent_control::InputPayload::from("follow up"),
        )
        .await;
    let interrupt = client
        .interrupt(fixture.app.event_ingest_service(), &target)
        .await;

    assert!(matches!(
        steer,
        Err(pontia_agent_control::DeliveryError::StateChanged(_))
    ));
    assert!(matches!(
        interrupt,
        Err(pontia_agent_control::DeliveryError::StateChanged(_))
    ));
    fixture.shutdown().await;
}

#[tokio::test]
async fn native_active_turn_drives_steer_and_interrupt() {
    let fixture = Fixture::new().await;
    fixture.dispatch_first_input().await;
    fixture
        .turns
        .lock()
        .await
        .push(json!({"id":"external-turn","status":"inProgress","items":[]}));
    let (target, client) = fixture.delivery().await;

    client
        .steer(
            fixture.app.event_ingest_service(),
            &target,
            &pontia_agent_control::InputPayload::from("follow up"),
        )
        .await
        .unwrap();
    client
        .interrupt(fixture.app.event_ingest_service(), &target)
        .await
        .unwrap();

    assert_eq!(
        fixture.inputs.lock().await[1]["expectedTurnId"],
        "external-turn"
    );
    assert_eq!(
        fixture.interrupts.lock().await[0]["turnId"],
        "external-turn"
    );
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
        pontia_agent_control::AgentBindingService::new(fixture.app.db())
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
                    Some(pontia_agent_control::interactions::InteractionState::Pending),
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
                    Some(pontia_agent_control::interactions::InteractionState::Expired),
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
    let fixture = Fixture::with_instructions(Some("caller-marker")).await;
    *fixture.configuration.lock().await = json!({"developer_instructions":"native-marker"});
    sqlx::query("DROP TABLE agent_profiles")
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
    let expected = format!(
        "native-marker\n\n{}\n\ncaller-marker",
        pontia_agent_control::standalone_session_instructions()
    );
    for request in requests.iter() {
        assert_eq!(
            request["params"]["config"]["developer_instructions"],
            expected
        );
    }
    drop(requests);
    fixture.shutdown().await;
}

#[tokio::test]
async fn native_title_notifications_update_sessions_without_overriding_manual_names() {
    use pontia_core::domain::EventType;
    let fixture = Fixture::new().await;
    fixture.dispatch_first_input().await;
    let mut events = fixture.app.agent_events().subscribe();
    let (shutdown, observer) = fixture.start_observer().await;
    let send = |method: &str, params: Value| {
        let mut params = params;
        params["threadId"] = json!("thread-one");
        fixture
            .notifications
            .send(json!({"method":method,"params":params}))
            .unwrap();
    };
    let title = || async {
        fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .title
    };

    send("thread/name/updated", json!({"threadName":"Native A"}));
    wait_for(
        &mut events,
        EventType::SessionTitleUpdated,
        Some("Native A"),
    )
    .await;
    assert_eq!(title().await.as_deref(), Some("Native A"));

    fixture
        .app
        .session_commands()
        .update_session(
            &fixture.session,
            serde_json::from_value(json!({"title":"Manual"})).unwrap(),
        )
        .await
        .unwrap();
    // A client title is ignored while the session has a manual name. The token usage
    // notification that follows proves the title notification was processed.
    send("thread/name/updated", json!({"threadName":"Native B"}));
    send(
        "thread/tokenUsage/updated",
        json!({"tokenUsage":{"last":{"totalTokens":10},"modelContextWindow":100}}),
    );
    wait_for(&mut events, EventType::SessionContextUsageUpdated, None).await;
    assert_eq!(title().await.as_deref(), Some("Manual"));

    fixture
        .app
        .session_commands()
        .update_session(
            &fixture.session,
            serde_json::from_value(json!({"title":null})).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(title().await, None);
    send("thread/name/updated", json!({"threadName":"Native C"}));
    wait_for(
        &mut events,
        EventType::SessionTitleUpdated,
        Some("Native C"),
    )
    .await;
    assert_eq!(title().await.as_deref(), Some("Native C"));
    shutdown.send(true).unwrap();
    observer.await.unwrap();
    fixture.shutdown().await;
}

#[tokio::test]
async fn native_title_snapshots_recover_changes_and_deduplicate_repeated_names() {
    use pontia_core::domain::EventType;
    let fixture = Fixture::new().await;
    *fixture.thread_name.lock().await = json!("Existing title");
    fixture.dispatch_first_input().await;
    assert_eq!(
        fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("Existing title")
    );
    for name in ["Renamed offline", "Renamed offline", "Existing title"] {
        fixture
            .app
            .session_commands()
            .terminate_session(&fixture.session)
            .await
            .unwrap();
        *fixture.thread_name.lock().await = json!(name);
        fixture
            .app
            .session_commands()
            .resume_session(&fixture.session, fixture.root.path())
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
                .title
                .as_deref(),
            Some(name)
        );
    }
    let titles = fixture
        .app
        .event_ingest_service()
        .list_events(&fixture.session)
        .await
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == EventType::SessionTitleUpdated)
        .map(|event| event.payload["title"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        titles,
        vec![
            json!("Existing title"),
            json!("Renamed offline"),
            json!("Existing title")
        ]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn malformed_client_title_reports_do_not_clear_a_session_name() {
    let fixture = Fixture::new().await;
    fixture
        .app
        .session_commands()
        .update_session(
            &fixture.session,
            serde_json::from_value(json!({"title":"Manual"})).unwrap(),
        )
        .await
        .unwrap();
    for payload in [json!({}), json!({"title":42}), json!({"title":{}})] {
        assert!(
            fixture
                .app
                .event_ingest_service()
                .report_client_fact(
                    &fixture.session,
                    None,
                    pontia_core::domain::EventType::SessionTitleUpdated,
                    payload,
                )
                .await
                .is_err()
        );
    }
    assert_eq!(
        fixture
            .app
            .queries()
            .get_session(&fixture.session)
            .await
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("Manual")
    );
    fixture.shutdown().await;
}

async fn wait_for(
    events: &mut tokio::sync::broadcast::Receiver<pontia_core::domain::DomainEvent>,
    event_type: pontia_core::domain::EventType,
    title: Option<&str>,
) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let event = events.recv().await.unwrap();
            if event.event_type == event_type
                && title.is_none_or(|title| event.payload["title"] == title)
            {
                break;
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn client_catalog_reports_native_default_and_configuration_override() {
    let fixture = Fixture::new().await;
    let service = pontia_agent_control::agent_clients::AgentClientService::new(
        fixture.app.db(),
        fixture.app.clients(),
    );
    let catalog = service
        .list_models("codex", "default", fixture.root.path())
        .await
        .unwrap();
    assert_eq!(catalog.default_model.as_deref(), Some("model-one"));
    *fixture.configuration.lock().await = json!({"model":"configured-model"});
    let catalog = service
        .list_models("codex", "default", fixture.root.path())
        .await
        .unwrap();
    assert_eq!(catalog.default_model.as_deref(), Some("configured-model"));
    assert_eq!(catalog.models[0].id, "model-one");
    fixture.shutdown().await;
}

#[tokio::test]
async fn observers_and_proxy_only_manage_sessions_on_their_own_server() {
    use crate::runtime::CodexServerTarget;
    let fixture = Fixture::new().await;
    let background = super::CodexObserver::for_target(
        &fixture.app,
        fixture.root.path().into(),
        CodexServerTarget::Background,
    );
    let user = CodexRuntime::existing(fixture.root.path()).await.unwrap();
    let mut request: CreateSessionRequest =
        serde_json::from_value(json!({"client_type":"codex","workspace":fixture.root.path()}))
            .unwrap();
    request.launch_mode = pontia_core::LaunchMode::Background;
    assert!(!background.has_active_sessions().await.unwrap());

    let outcome = fixture
        .app
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = outcome.session_id().unwrap();
    let service = super::CodexService::new(fixture.app.event_ingest_service());
    let thread = json!({"thread":{"id":"background-thread","cwd":fixture.root.path(),"canAcceptDirectInput":true,"status":{"type":"idle"}}});
    pontia_agent_control::AgentBindingService::new(fixture.app.db())
        .upsert_binding(pontia_agent_control::UpsertAgentBindingRequest {
            session_id: session.into(),
            client_type: "codex".into(),
            launch_cwd: fixture.root.path().display().to_string(),
            client_session_key: "background-thread".into(),
            client_session_file: None,
            metadata: json!({}),
        })
        .await
        .unwrap();
    assert!(
        CodexRuntime::for_session(&fixture.app.db(), session)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        service
            .adopt_opened_thread(
                &user,
                crate::proxy::ThreadOperation::Resume,
                &thread,
                &json!({}),
                None,
                None
            )
            .await,
        Err(pontia_core::Error::StateConflict(_))
    ));
    let mut child = thread.clone();
    child["thread"]["id"] = json!("unbound-child");
    child["thread"]["forkedFromId"] = json!("background-thread");
    assert!(matches!(
        service
            .adopt_opened_thread(
                &user,
                crate::proxy::ThreadOperation::Fork,
                &child,
                &json!({}),
                None,
                None
            )
            .await,
        Err(pontia_core::Error::StateConflict(_))
    ));
    assert!(
        pontia_agent_control::AgentBindingService::new(fixture.app.db())
            .binding_for_client_session("codex", "unbound-child")
            .await
            .unwrap()
            .is_none()
    );
    assert!(background.has_active_sessions().await.unwrap());
    assert!(user.subscription(session).await.is_none());
    fixture.shutdown().await;
}
