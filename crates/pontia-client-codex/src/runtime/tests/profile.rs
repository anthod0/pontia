use super::*;
use pontia_application::{
    AgentBindingService, AgentProfileService, runtime::ControlTarget, turns::InputIntent,
};

async fn profile(app: &AppState, id: &str, prompt: &str) {
    AgentProfileService::new(app.db()).with_clients(app.event_ingest_service().clients())
        .create_profile(serde_json::from_value(json!({"profile_id":id,"version":"1","name":id,"supported_client_types":["codex"],"agent_kind":"executor","system_prompt_template":prompt})).unwrap()).await.unwrap();
}

async fn create(app: &AppState, root: &Path, profile: &str) -> String {
    app.session_commands()
        .create_session(
            serde_json::from_value(
                json!({"client_type":"codex","workspace":root,"execution_profile_id":profile}),
            )
            .unwrap(),
        )
        .await
        .unwrap()
        .session_id()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn profile_instructions_survive_inputs_reconnects_and_tui_target_switches() {
    let root = tempfile::tempdir().unwrap();
    let _tuis = tui::TuiCleanup(root.path().into());
    let requests = Arc::new(Mutex::new(Vec::<Value>::new()));
    let wire_requests = requests.clone();
    let listener = UnixListener::bind(root.path().join("daemon.sock")).unwrap();
    let cwd = root.path().to_path_buf();
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let requests = wire_requests.clone();
            let cwd = cwd.clone();
            tokio::spawn(async move {
                let mut wire = accept_async(stream).await.unwrap();
                while let Some(Ok(Message::Text(frame))) = wire.next().await {
                    let request: Value = serde_json::from_str(&frame).unwrap();
                    if request.get("id").is_none() {
                        continue;
                    }
                    requests.lock().await.push(request.clone());
                    let result = match request["method"].as_str().unwrap() {
                        "initialize" => {
                            json!({"userAgent":"pontia/0.156.1","codexHome":cwd,"platformFamily":"unix","platformOs":"linux"})
                        }
                        "thread/start"
                            if request["params"]["developerInstructions"] == "reject" =>
                        {
                            wire.send(Message::Text(json!({"id":request["id"],"error":{"code":-32602,"message":"instructions rejected"}}).to_string().into())).await.unwrap();
                            continue;
                        }
                        "thread/start" | "thread/resume" | "thread/read" => {
                            json!({"thread":{"id":request["params"]["threadId"].as_str().unwrap_or("profile-thread"),"cwd":cwd,"canAcceptDirectInput":true,"status":{"type":"idle"}},"model":"test"})
                        }
                        "thread/turns/list" | "thread/list" => json!({"data":[],"nextCursor":null}),
                        "turn/start" => json!({"turn":{"id":"native-turn"}}),
                        method => panic!("unexpected method {method}"),
                    };
                    if wire
                        .send(Message::Text(
                            json!({"id":request["id"],"result":result})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
    });
    let mut runtime = lifecycle::connect(root.path()).await.unwrap();
    registry().lock().await.remove(&runtime.root);
    Arc::get_mut(&mut runtime).unwrap().tui_command = tui::launcher(root.path());
    registry()
        .lock()
        .await
        .insert(runtime.root.clone(), runtime.clone());
    let app = tui::app(root.path()).await;
    profile(&app, "reviewer", "PROFILE_ONLY_MARKER").await;
    let session = create(&app, root.path(), "reviewer").await;
    let service = crate::CodexService::new(app.event_ingest_service());
    assert!(
        !requests
            .lock()
            .await
            .iter()
            .any(|r| r["method"] == "thread/start")
    );
    let submit = |session: String| {
        let service = service.clone();
        let pool = app.db();
        async move {
            let target = ControlTarget::resolve(&pool, &session, None).await.unwrap();
            service
                .submit(
                    &target,
                    "What is the configured response?",
                    None,
                    &InputIntent::Start,
                )
                .await
        }
    };
    submit(session.clone()).await.unwrap();
    assert!(
        AgentProfileService::new(app.db())
            .confirm_codex_configuration(&session, "another-thread")
            .await
            .is_err()
    );
    // An edit to the referenced version must not alter either input surface.
    sqlx::query(
        "UPDATE execution_profiles SET system_prompt_template='EDITED' WHERE profile_id='reviewer'",
    )
    .execute(&app.db())
    .await
    .unwrap();
    submit(session.clone()).await.unwrap();
    let gateway = runtime.gateway("profile-peer").await.unwrap();
    let peer = Connection::connect(Path::new(gateway.trim_start_matches("unix://")))
        .await
        .unwrap();
    peer.call("thread/resume", json!({"threadId":"profile-thread"}))
        .await
        .unwrap();
    peer.call("turn/start", json!({"threadId":"profile-thread","input":[{"type":"text","text":"What is the configured response?"}]})).await.unwrap();
    for params in [
        json!({"threadId":"profile-thread","developerInstructions":"override"}),
        json!({"threadId":"profile-thread","config":{"developer_instructions":"override"}}),
    ] {
        assert!(peer.call("thread/resume", params).await.is_err());
    }
    // A different thread reached through this TUI never inherits its owner's Profile.
    peer.call("thread/resume", json!({"threadId":"external-thread"}))
        .await
        .unwrap();
    let captured = requests.lock().await.clone();
    for request in &captured {
        let method = request["method"].as_str().unwrap();
        if method == "thread/start"
            || (method == "thread/resume" && request["params"]["threadId"] == "profile-thread")
        {
            assert_eq!(
                request["params"]["developerInstructions"],
                "PROFILE_ONLY_MARKER"
            );
        }
        if method == "turn/start" {
            assert_eq!(
                request["params"]["input"][0]["text"],
                "What is the configured response?"
            );
        }
    }
    assert!(
        captured
            .iter()
            .filter(|r| r["params"]["threadId"] == "external-thread")
            .all(|r| r["params"]["developerInstructions"].is_null())
    );
    assert_eq!(
        captured
            .iter()
            .filter(|r| r["method"] == "thread/start")
            .count(),
        1
    );
    CodexRuntime::shutdown(root.path()).await;
    let replacement = lifecycle::connect(root.path()).await.unwrap();
    service.prepare_connection(&replacement).await.unwrap();
    let gateway = replacement.gateway("recovery-peer").await.unwrap();
    let recovered = Connection::connect(Path::new(gateway.trim_start_matches("unix://")))
        .await
        .unwrap();
    recovered
        .call("thread/resume", json!({"threadId":"profile-thread"}))
        .await
        .unwrap();
    assert_eq!(
        requests.lock().await.last().unwrap()["params"]["developerInstructions"],
        "PROFILE_ONLY_MARKER"
    );

    // Losing configuration evidence fails closed on both control surfaces.
    sqlx::query("UPDATE runtime_bindings SET adapter_details=json_remove(adapter_details,'$.codex_profile_thread') WHERE session_id=?").bind(&session).execute(&app.db()).await.unwrap();
    let target = ControlTarget::resolve(&app.db(), &session, None)
        .await
        .unwrap();
    assert!(
        service
            .submit(&target, "Next", None, &InputIntent::Start)
            .await
            .is_err()
    );
    assert!(service.open_tui(&session).await.is_err());
    let gateway = replacement.gateway("reconnected-peer").await.unwrap();
    let peer = Connection::connect(Path::new(gateway.trim_start_matches("unix://")))
        .await
        .unwrap();
    for method in ["thread/resume", "turn/start", "turn/steer"] {
        let error = peer
            .call(method, json!({"threadId":"profile-thread","input":[]}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("unverified"));
    }
    profile(&app, "rejected", "reject").await;
    let rejected = create(&app, root.path(), "rejected").await;
    let target = ControlTarget::resolve(&app.db(), &rejected, None)
        .await
        .unwrap();
    let error = service
        .submit(&target, "Next", None, &InputIntent::Start)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("instructions rejected"));
    assert!(
        AgentBindingService::new(app.db())
            .binding_for_session(&rejected)
            .await
            .unwrap()
            .is_none()
    );
    CodexRuntime::shutdown(root.path()).await;
    server.abort();
}

// Requires a caller-owned daemon. Workspace/database/TUI cleanup is confined to
// this temporary root; neither the daemon nor another native thread is stopped.
#[tokio::test]
#[ignore = "requires an externally running Codex 0.156.1 daemon and model access"]
async fn native_profile_controls_dashboard_tui_and_resumed_thread() {
    let root = tempfile::tempdir().unwrap();
    let _tuis = tui::TuiCleanup(root.path().into());
    let runtime = CodexRuntime::ensure(root.path()).await.unwrap();
    let app = tui::app(root.path()).await;
    let marker = format!(
        "PROFILE_{}",
        root.path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace('.', "_")
    );
    profile(
        &app,
        "native-profile",
        &format!("For every user message, reply with exactly {marker}. Do not use tools."),
    )
    .await;
    let session = create(&app, root.path(), "native-profile").await;
    let service = crate::CodexService::new(app.event_ingest_service());
    let target = ControlTarget::resolve(&app.db(), &session, None)
        .await
        .unwrap();
    service
        .submit(
            &target,
            "What is your configured response?",
            None,
            &InputIntent::Start,
        )
        .await
        .unwrap();
    let binding = AgentBindingService::new(app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    let native = runtime.connection().await.unwrap();
    assert_native_reply(&native, &binding.client_session_key, 1, &marker).await;
    let (socket, pane) = runtime.tui_pane(&session).unwrap();
    for args in [
        vec![
            "send-keys",
            "-t",
            &pane,
            "-l",
            "What is your configured response?",
        ],
        vec!["send-keys", "-t", &pane, "Enter"],
    ] {
        assert!(
            std::process::Command::new("tmux")
                .args(["-S", &socket])
                .args(args)
                .status()
                .unwrap()
                .success()
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_native_reply(&native, &binding.client_session_key, 2, &marker).await;
    // Remove only this test's TUI, then archive its thread to exercise a cold resume.
    assert!(
        std::process::Command::new("tmux")
            .args(["-S", &socket, "kill-pane", "-t", &pane])
            .status()
            .unwrap()
            .success()
    );
    native
        .call(
            "thread/archive",
            json!({"threadId":binding.client_session_key}),
        )
        .await
        .unwrap();
    service
        .resume(
            &ControlTarget::resolve(&app.db(), &session, None)
                .await
                .unwrap(),
        )
        .await
        .unwrap();
    service
        .submit(
            &ControlTarget::resolve(&app.db(), &session, None)
                .await
                .unwrap(),
            "What is your configured response?",
            None,
            &InputIntent::Start,
        )
        .await
        .unwrap();
    assert_native_reply(&native, &binding.client_session_key, 3, &marker).await;
    let resumed = AgentBindingService::new(app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resumed.client_session_key, binding.client_session_key);
    let records = std::fs::read_to_string(resumed.client_session_file.unwrap()).unwrap();
    let mut developer_markers = 0;
    for line in records.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        if record["type"] == "response_item" && record["payload"]["role"] == "developer" {
            developer_markers += record["payload"]["content"]
                .to_string()
                .matches(&marker)
                .count();
        }
    }
    assert_eq!(
        developer_markers, 1,
        "the native developer instruction must be injected once, without reconnect accumulation"
    );
    eprintln!(
        "Native Profile evidence: daemon={}, thread={}, three matching replies, developer marker count={developer_markers}",
        runtime.connection.server_version, binding.client_session_key
    );
    CodexRuntime::shutdown(root.path()).await;
}

async fn assert_native_reply(connection: &Connection, thread: &str, count: usize, marker: &str) {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let result = connection
                .call("thread/turns/list", json!({"threadId":thread,"limit":20}))
                .await
                .unwrap();
            let turns = result["data"].as_array().unwrap();
            assert!(turns.len() <= count, "input must not be replayed");
            if turns.len() == count && turns.iter().all(|turn| turn["status"] == "completed") {
                for turn in turns {
                    assert!(
                        turn["items"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|item| item["type"] == "agentMessage"
                                && item["text"]
                                    .as_str()
                                    .is_some_and(|text| text.trim() == marker)),
                        "{turn}"
                    );
                    assert!(
                        turn["items"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|item| item["type"] == "userMessage")
                            .all(|item| !item.to_string().contains(marker))
                    );
                }
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
}
