use super::*;

#[tokio::test]
async fn first_input_resume_and_fork_have_the_bound_session_environment() {
    let f = Fixture::new().await;
    *f.configuration.lock().await = json!({"shell_environment_policy":{
        "set":{"NATIVE":"native","PONTIA_SESSION_ID":"wrong-native"},
        "filters":{"PATH":"include","DROP":"exclude"}
    }});
    let mut cli = f.client().await;
    let started = rpc(
        &mut cli,
        "thread/start",
        json!({"config":{
            "shell_environment_policy.set":{"KEEP":"request","PONTIA_SESSION_ID":"wrong-request"},
            "model_reasoning_effort":"high"
        }}),
    )
    .await;
    let native = started["result"]["thread"]["id"].as_str().unwrap();
    rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
    // The binding must exist before the first input reaches the native server.
    let session = AgentBindingService::new(f.app.db())
        .binding_for_client_session("codex", native)
        .await
        .unwrap()
        .unwrap()
        .session_id;
    let config = f.threads.lock().await[native].config.clone();
    assert_eq!(
        config["shell_environment_policy"]["set"],
        json!({"NATIVE":"native","KEEP":"request","PONTIA_SESSION_ID":session})
    );
    assert_eq!(
        config["shell_environment_policy"]["filters"],
        json!({"PATH":"include","DROP":"exclude","PONTIA_SESSION_ID":"include"})
    );
    assert_eq!(config["model_reasoning_effort"], "high");
    f.session(native, "busy").await;
    f.complete(native).await;
    f.session(native, "idle").await;
    rpc(&mut cli, "thread/resume", json!({"threadId":native})).await;
    assert_eq!(
        f.threads.lock().await[native].config["shell_environment_policy"]["set"]["PONTIA_SESSION_ID"],
        session
    );
    let fork = rpc(&mut cli, "thread/fork", json!({"threadId":native})).await;
    let child = fork["result"]["thread"]["id"].as_str().unwrap();
    let child_session = AgentBindingService::new(f.app.db())
        .binding_for_client_session("codex", child)
        .await
        .unwrap()
        .unwrap()
        .session_id;
    assert_ne!(session, child_session);
    assert_eq!(
        f.threads.lock().await[child].config["shell_environment_policy"]["set"]["PONTIA_SESSION_ID"],
        child_session
    );
    f.stop().await;
}

#[tokio::test]
async fn opening_a_thread_does_not_block_another_threads_input_or_output() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let opened = rpc(&mut cli, "thread/start", json!({})).await;
    let native = opened["result"]["thread"]["id"].as_str().unwrap();
    let permit = f.config_gate.acquire().await.unwrap();
    cli.send(Message::text(
        json!({"id":2,"method":"thread/start","params":{"cwd":f.root.path()}}).to_string(),
    ))
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.config_gate.available_permits() != 0
            || f.calls
                .lock()
                .await
                .iter()
                .filter(|(_, method, _)| method == "config/read")
                .count()
                < 2
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let turn = rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
    assert_eq!(turn["result"]["turn"]["status"], "inProgress");
    drop(permit);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let Message::Text(text) = cli.next().await.unwrap().unwrap() else {
                continue;
            };
            let value: Value = serde_json::from_str(&text).unwrap();
            if value["id"] == 2 {
                assert!(value.get("result").is_some(), "{value}");
                break;
            }
        }
    })
    .await
    .unwrap();
    f.stop().await;
}

#[tokio::test]
async fn concurrent_first_resumes_share_one_environment_identity() {
    let f = Fixture::new().await;
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let opened = runtime
        .connection()
        .await
        .unwrap()
        .call("thread/fork", json!({}))
        .await
        .unwrap();
    let native = opened["thread"]["id"].as_str().unwrap();
    let mut first = f.client().await;
    let mut second = f.client().await;
    let (a, b) = tokio::join!(
        rpc(&mut first, "thread/resume", json!({"threadId":native})),
        rpc(&mut second, "thread/resume", json!({"threadId":native}))
    );
    assert!(a.get("error").is_none() && b.get("error").is_none());
    let session = f.session(native, "idle").await;
    assert_eq!(
        f.threads.lock().await[native].config["shell_environment_policy"]["set"]["PONTIA_SESSION_ID"],
        session
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&f.app.db())
        .await
        .unwrap();
    assert_eq!(count, 1);
    f.stop().await;
}

#[tokio::test]
async fn proxy_resume_keeps_the_bound_restricted_session_configuration() {
    let f = Fixture::new().await;
    *f.configuration.lock().await = json!({"mcp_servers":{"workspace":{"command":"fixture"}}});
    let created = f
        .app
        .session_commands()
        .create_session(
            serde_json::from_value(json!({
                "client_type":"codex", "workspace":f.root.path(), "mode":"restricted"
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    let session = created.session_id().unwrap();
    f.app
        .turn_commands()
        .create_and_dispatch_turn(session, "hello".into(), json!({}))
        .await
        .unwrap();
    let native = AgentBindingService::new(f.app.db())
        .binding_for_session(session)
        .await
        .unwrap()
        .unwrap()
        .client_session_key;
    f.complete(&native).await;
    f.session(&native, "idle").await;
    let path = f.root.path().join("stored.jsonl");
    f.threads.lock().await.get_mut(&native).unwrap().value["path"] = json!(path);
    let mut cli = f.client().await;
    let reply = rpc(
        &mut cli,
        "thread/resume",
        json!({"threadId":native, "path":path, "config":{
            "features.multi_agent":true, "mcp_servers.extra.enabled":true, "skills.config":[]
        }}),
    )
    .await;
    assert_eq!(reply["result"]["thread"]["id"], native);
    let config = f.threads.lock().await[&native].config.clone();
    assert_eq!(config["approval_policy"], "never");
    assert_eq!(config["sandbox_mode"], "danger-full-access");
    assert_eq!(
        config["shell_environment_policy.set"]["PONTIA_SESSION_ID"],
        session
    );
    assert_eq!(
        config["mcp_servers"],
        json!({"workspace":{"enabled":false}})
    );
    assert!(config.get("mcp_servers.extra.enabled").is_none());
    assert_eq!(
        config["skills.config"],
        json!([{"name":"workspace-skill","enabled":false}])
    );
    assert_eq!(config["features.multi_agent"], false);
    assert_eq!(config["web_search"], "disabled");
    for extra in [
        json!({"ephemeral":true}),
        json!({"path":"another.jsonl"}),
        json!({}),
    ] {
        if extra == json!({}) {
            *f.configuration.lock().await = json!({});
        }
        let mut cli = f.client().await;
        let mut params = extra;
        params["threadId"] = json!(native);
        cli.send(Message::text(
            json!({"id":2,"method":"thread/resume","params":params}).to_string(),
        ))
        .await
        .unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(5), cli.next())
            .await
            .unwrap();
        assert!(!matches!(frame, Some(Ok(Message::Text(_)))));
    }
    assert_eq!(
        f.calls
            .lock()
            .await
            .iter()
            .filter(|(pontia, method, _)| !pontia && method == "thread/resume")
            .count(),
        1
    );
    f.stop().await;
}
