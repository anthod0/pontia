use std::{collections::BTreeMap, time::Duration};

use pontia_application::{
    AppState, CreateSessionRequest, SubmitInboxMessageRequest,
    client_contract::ClientServicePhase,
    clients::{ClientRegistry, ClientServiceTasks},
};
use pontia_client_acp::identity;
use pontia_config::{AcpAgentConfig, AcpConfig};
use pontia_core::{Error, Result};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};

struct Harness {
    app: AppState,
    tasks: Option<ClientServiceTasks>,
    root: tempfile::TempDir,
}

impl Harness {
    async fn new(flags: &[(&str, &str)]) -> Self {
        Self::with_command(flags, "python3").await
    }

    async fn with_command(flags: &[(&str, &str)], command: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut env = BTreeMap::from([
            ("ACP_FIXTURE_ROOT".into(), root.path().display().to_string()),
            ("ACP_LITERAL".into(), "${NOT_EXPANDED} ~ *".into()),
        ]);
        env.extend(
            flags
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string())),
        );
        let config = AcpAgentConfig {
            name: Some("Controlled Agent".into()),
            command: command.into(),
            args: vec![
                format!("{}/tests/fixtures/agent.py", env!("CARGO_MANIFEST_DIR")),
                "argument with spaces; not a shell".into(),
            ],
            env,
        };
        let mut clients = ClientRegistry::default();
        clients.register(pontia_client_acp::registration(AcpConfig {
            agents: BTreeMap::from([("first".into(), config.clone()), ("second".into(), config)]),
        }));
        let db = connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("test.db").display()
        ))
        .await
        .unwrap();
        run_migrations(&db).await.unwrap();
        let app = AppState::builder(db, root.path().into())
            .clients(clients.clone())
            .build();
        let tasks = clients
            .start_services(app.clone(), ClientServicePhase::Transport)
            .await
            .unwrap();
        Self {
            app,
            root,
            tasks: Some(tasks),
        }
    }

    async fn create(&self, agent: &str) -> Result<String> {
        let request: CreateSessionRequest = serde_json::from_value(json!({
            "client_type":"acp", "workspace":self.root.path(), "metadata":{"acp_agent_id":agent}
        }))
        .unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            self.app.session_commands().create_session(request),
        )
        .await
        .expect("Session creation must not hang")?;
        Ok(result.session_id().unwrap().into())
    }

    fn release(&self, marker: &str) {
        std::fs::write(self.root.path().join(marker), b"").unwrap();
    }

    fn wire(&self) -> Vec<Value> {
        std::fs::read_to_string(self.root.path().join("wire.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn requests(&self, method: &str) -> usize {
        self.wire()
            .iter()
            .filter(|item| item["method"] == method)
            .count()
    }

    async fn events(&self, session: &str, kind: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id=? AND event_type=?")
            .bind(session)
            .bind(kind)
            .fetch_one(&self.app.db())
            .await
            .unwrap()
    }

    async fn wait_turn(&self, session: &str, turn: &str, state: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if self
                    .app
                    .queries()
                    .get_turn(session, turn)
                    .await
                    .unwrap()
                    .is_some_and(|view| view.state == state)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Turn did not become {state}"));
    }

    async fn wait_runtime_exited(&self, session: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let state: Option<String> =
                    sqlx::query_scalar("SELECT state FROM session_runtimes WHERE session_id=?")
                        .bind(session)
                        .fetch_optional(&self.app.db())
                        .await
                        .unwrap();
                if state.as_deref() == Some("exited") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn stop(mut self) {
        self.app.shutdown().notify();
        tokio::time::timeout(Duration::from_secs(5), self.tasks.take().unwrap().join())
            .await
            .unwrap()
            .unwrap();
        self.app.db().close().await;
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.app.shutdown().notify();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn directly_executes_the_selected_symlink_instead_of_its_canonical_target() {
    let root = tempfile::tempdir().unwrap();
    let python = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("python3"))
        .find(|path| path.is_file())
        .unwrap()
        .canonicalize()
        .unwrap();
    let selected = root.path().join("environment-python");
    std::os::unix::fs::symlink(python, &selected).unwrap();
    let h = Harness::with_command(&[], selected.to_str().unwrap()).await;
    h.create("first").await.unwrap();
    let startup = h
        .wire()
        .into_iter()
        .find(|frame| frame.get("startup").is_some())
        .unwrap();
    assert_eq!(startup["startup"]["executable"], selected.to_str().unwrap());
    h.stop().await;
}

#[tokio::test]
async fn explicit_retry_after_rejected_initialization_reuses_the_only_runtime() {
    let h = Harness::new(&[("ACP_NO_LOAD", "1")]).await;
    assert!(h.create("first").await.is_err());
    let session: String = sqlx::query_scalar("SELECT session_id FROM sessions")
        .fetch_one(&h.app.db())
        .await
        .unwrap();
    h.wait_runtime_exited(&session).await;
    let runtime: String =
        sqlx::query_scalar("SELECT runtime_id FROM session_runtimes WHERE session_id=?")
            .bind(&session)
            .fetch_one(&h.app.db())
            .await
            .unwrap();
    let entry = h.app.clients().get("acp").unwrap().clone();
    let retry = entry
        .session
        .unwrap()
        .provision(
            h.app.event_ingest_service(),
            h.root.path(),
            pontia_runtime::RuntimeStartRequest {
                session_id: session.clone(),
                runtime_id: Some("must-not-create-another-runtime".into()),
                client_type: "acp".into(),
                workspace: Some(h.root.path().display().to_string()),
                workspace_name: None,
                handle: None,
                role: None,
                start_command: None,
                environment: BTreeMap::new(),
            },
        )
        .await;
    assert!(matches!(retry, Err(Error::CapabilityUnavailable(_))));
    let runtimes: Vec<String> =
        sqlx::query_scalar("SELECT runtime_id FROM session_runtimes WHERE session_id=?")
            .bind(&session)
            .fetch_all(&h.app.db())
            .await
            .unwrap();
    assert_eq!(runtimes, [runtime]);
    assert_eq!(h.requests("initialize"), 2);
    h.stop().await;
}

#[tokio::test]
async fn close_fences_late_updates_without_fabricating_an_active_turn_result() {
    let h = Harness::new(&[("ACP_UPDATE_AFTER_CLOSE", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    for event in [
        "turn.started",
        "turn.completed",
        "turn.failed",
        "turn.interrupted",
    ] {
        assert_eq!(h.events(&session, event).await, 0);
    }
    assert!(
        h.app
            .live_output()
            .snapshot(&session, &turn.turn_id)
            .is_none()
    );
    assert!(
        !h.app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .accept_task
    );
    h.stop().await;
}

#[tokio::test]
async fn capabilities_remain_unavailable_until_session_setup_finishes() {
    let h = Harness::new(&[("ACP_HOLD_NEW", "1")]).await;
    let app = h.app.clone();
    let request: CreateSessionRequest = serde_json::from_value(
        json!({"client_type":"acp","workspace":h.root.path(),"metadata":{"acp_agent_id":"first"}}),
    )
    .unwrap();
    let creating =
        tokio::spawn(async move { app.session_commands().create_session(request).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while h.requests("session/new") == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let session: String = sqlx::query_scalar("SELECT session_id FROM sessions")
        .fetch_one(&h.app.db())
        .await
        .unwrap();
    let before = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(!before.capabilities.accept_task);
    assert!(!before.capabilities.stream_output);
    assert_eq!(h.events(&session, "session.ready").await, 0);
    h.release("release-new");
    creating.await.unwrap().unwrap();
    assert!(
        h.app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .accept_task
    );
    h.stop().await;
}

#[tokio::test]
async fn unknown_extension_update_cannot_invent_started_or_break_the_connection() {
    let h = Harness::new(&[("ACP_EXTENSION_UPDATE", "1"), ("ACP_NO_UPDATES", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    tokio::time::timeout(Duration::from_secs(3), async {
        while !h
            .wire()
            .iter()
            .any(|frame| frame["id"] == "extension-barrier" && frame.get("error").is_some())
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(h.events(&session, "turn.started").await, 0);
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    assert!(
        h.app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .accept_task
    );
    h.stop().await;
}

#[tokio::test]
async fn confirmed_final_response_is_not_lost_when_peer_exits_immediately_after_it() {
    let h = Harness::new(&[("ACP_EXIT_AFTER_RESULT", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    h.wait_runtime_exited(&session).await;
    assert_eq!(h.events(&session, "session.exited").await, 0);
    h.stop().await;
}

#[tokio::test]
async fn successful_close_response_remains_authoritative_after_immediate_eof() {
    let h = Harness::new(&[("ACP_EXIT_AFTER_CLOSE", "1")]).await;
    let session = h.create("first").await.unwrap();
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    assert_eq!(h.events(&session, "session.exited").await, 1);
    h.stop().await;
}

#[tokio::test]
async fn partial_prompt_write_returns_unknown_without_retry_or_dispatch_failed_fact() {
    let h = Harness::new(&[("ACP_BREAK_INPUT", "1")]).await;
    let session = h.create("first").await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        h.app
            .turn_commands()
            .create_and_dispatch_turn(&session, "x".repeat(262144), json!({})),
    )
    .await
    .unwrap();
    assert!(
        matches!(result, Err(Error::ControlUnknown(_))),
        "{result:?}"
    );
    h.wait_runtime_exited(&session).await;
    for event in [
        "turn.started",
        "turn.dispatch_failed",
        "turn.failed",
        "turn.completed",
    ] {
        assert_eq!(h.events(&session, event).await, 0);
    }
    assert_eq!(h.requests("session/new"), 1);
    h.stop().await;
}

#[tokio::test]
async fn missing_or_invalid_selection_is_rejected_without_starting_a_peer() {
    let h = Harness::new(&[]).await;
    for selection in [Value::Null, json!(""), json!(123), json!("unknown")] {
        let request: CreateSessionRequest = serde_json::from_value(json!({"client_type":"acp","workspace":h.root.path(),"metadata":{"acp_agent_id":selection}})).unwrap();
        assert!(
            h.app
                .session_commands()
                .create_session(request)
                .await
                .is_err()
        );
    }
    assert_eq!(h.requests("initialize"), 0);
    h.stop().await;
}

#[tokio::test]
async fn reprovisioning_a_bound_session_cannot_replace_native_identity_or_add_runtime() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    let entry = h.app.clients().get("acp").unwrap().clone();
    let result = entry
        .session
        .unwrap()
        .provision(
            h.app.event_ingest_service(),
            h.root.path(),
            pontia_runtime::RuntimeStartRequest {
                session_id: session.clone(),
                runtime_id: Some("another-runtime".into()),
                client_type: "acp".into(),
                workspace: Some(h.root.path().display().to_string()),
                workspace_name: None,
                handle: None,
                role: None,
                start_command: None,
                environment: BTreeMap::new(),
            },
        )
        .await;
    assert!(matches!(result, Err(Error::StateConflict(_))));
    let after = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.runtimes, view.runtimes);
    assert_eq!(h.requests("session/new"), 1);
    h.stop().await;
}

#[test]
fn composite_identity_is_reversible_and_namespaces_native_ids() {
    let key = identity::encode("a\"\\雪", " s\n\t ").unwrap();
    assert_eq!(identity::decode(&key).unwrap(), ["a\"\\雪", " s\n\t "]);
    assert_ne!(
        identity::encode("first", "same").unwrap(),
        identity::encode("second", "same").unwrap()
    );
    assert_eq!(
        identity::encode("first", "native").unwrap(),
        r#"["first","native"]"#
    );
    for invalid in [
        r#"["a", "b"]"#,
        r#"["a","b"]
"#,
        r#"["a"]"#,
        r#"["a","b","c"]"#,
        r#"["a",1]"#,
        r#"["","b"]"#,
        r#"["a",""]"#,
        r#"["\u0061","b"]"#,
    ] {
        assert!(identity::decode(invalid).is_err(), "accepted {invalid}");
    }
}

#[tokio::test]
async fn creation_immediately_initializes_peer_and_persists_one_identity_and_runtime() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.state, "idle");
    assert!(view.capabilities.accept_task);
    assert!(!view.capabilities.interrupt);
    assert!(!view.capabilities.timeline);
    assert_eq!(view.runtimes.len(), 1);
    assert_eq!(view.runtimes[0].role, "acp");
    assert_eq!(view.runtimes[0].state, "running");
    assert!(
        view.runtimes[0]
            .start_command
            .as_ref()
            .unwrap()
            .contains("argument with spaces; not a shell")
    );
    let binding = pontia_application::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(binding.client_session_key, r#"["first","native-id"]"#);
    assert_eq!(
        binding.metadata,
        json!({"agent_info":{"name":"fixture","title":"Test Agent","version":"arbitrary-release","_meta":{"build":"fixture-build"}}})
    );
    let startup = h
        .wire()
        .into_iter()
        .find(|value| value.get("startup").is_some())
        .unwrap();
    assert_eq!(startup["startup"]["cwd"], h.root.path().to_str().unwrap());
    assert_eq!(
        startup["startup"]["argv"],
        json!(["argument with spaces; not a shell"])
    );
    assert_eq!(startup["startup"]["literal"], "${NOT_EXPANDED} ~ *");
    assert_eq!(h.requests("session/new"), 1);
    assert_eq!(h.requests("session/prompt"), 0);
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    assert_eq!(
        h.app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "exited"
    );
    assert_eq!(h.events(&session, "turn.completed").await, 0);
    h.stop().await;
}

#[tokio::test]
async fn missing_minimum_capabilities_rejects_before_new_and_cleans_peer() {
    for flag in ["ACP_NO_LOAD", "ACP_NO_CLOSE"] {
        let h = Harness::new(&[(flag, "1")]).await;
        assert!(matches!(
            h.create("first").await,
            Err(Error::CapabilityUnavailable(_))
        ));
        assert_eq!(h.requests("session/new"), 0);
        let session: String = sqlx::query_scalar("SELECT session_id FROM sessions")
            .fetch_one(&h.app.db())
            .await
            .unwrap();
        assert!(
            pontia_application::AgentBindingService::new(h.app.db())
                .binding_for_session(&session)
                .await
                .unwrap()
                .is_none()
        );
        h.wait_runtime_exited(&session).await;
        assert!(
            !h.app
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap()
                .capabilities
                .accept_task
        );
        assert_eq!(h.events(&session, "session.ready").await, 0);
        h.stop().await;
    }
}

#[tokio::test]
async fn input_is_async_streams_without_persisting_output_and_uses_standard_stop_results() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    for (index, reason, expected) in [
        (1, "end_turn", "completed"),
        (2, "max_tokens", "failed"),
        (3, "max_turn_requests", "failed"),
        (4, "refusal", "failed"),
        (5, "cancelled", "interrupted"),
    ] {
        let turn = tokio::time::timeout(
            Duration::from_secs(3),
            h.app
                .turn_commands()
                .create_and_dispatch_turn(&session, reason.into(), json!({})),
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
        assert_eq!(h.events(&session, "turn.started").await, index - 1);
        h.release(&format!("update-{index}"));
        h.wait_turn(&session, &turn.turn_id, "running").await;
        assert!(
            h.app
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap()
                .capabilities
                .interrupt
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if h.app
                    .live_output()
                    .snapshot(&session, &turn.turn_id)
                    .is_some()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let snapshot = h
            .app
            .live_output()
            .snapshot(&session, &turn.turn_id)
            .unwrap();
        assert!(
            matches!(&snapshot.items[0], pontia_application::LiveOutputItem::AssistantText { text, .. } if text == "streamed answer")
        );
        h.release(&format!("release-{index}"));
        h.wait_turn(&session, &turn.turn_id, expected).await;
        assert!(
            !h.app
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap()
                .capabilities
                .interrupt
        );
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE payload LIKE '%streamed answer%'")
            .fetch_one(&h.app.db())
            .await
            .unwrap();
    assert_eq!(count, 0);
    assert_eq!(h.requests("session/prompt"), 5);
    assert!(!h.wire().iter().any(|item| item["overlap"] == true));
    h.stop().await;
}

#[tokio::test]
async fn cancel_keeps_prompt_active_and_inbox_queued_until_final_response() {
    let h = Harness::new(&[("ACP_CANCEL_UPDATE", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.wait_turn(&session, &turn.turn_id, "running").await;
    h.app
        .turn_commands()
        .interrupt_turn(&session, &turn.turn_id)
        .await
        .unwrap();
    h.app
        .inbox_commands()
        .submit_message(
            &session,
            SubmitInboxMessageRequest {
                input: "end_turn".into(),
                delivery_policy: "after_idle".into(),
                branch_target_turn_id: None,
                metadata: json!({}),
            },
        )
        .await
        .unwrap();
    assert_eq!(h.events(&session, "turn.interrupted").await, 0);
    assert_eq!(h.requests("session/prompt"), 1);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = h.app.live_output().snapshot(&session, &turn.turn_id);
            if snapshot.is_some_and(|snapshot| matches!(&snapshot.items[0], pontia_application::LiveOutputItem::AssistantText { text, .. } if text.ends_with(" after cancel"))) { break; }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }).await.unwrap();
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "interrupted").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while h.requests("session/prompt") != 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        h.app
            .turn_commands()
            .interrupt_turn(&session, &turn.turn_id)
            .await
            .is_err()
    );
    assert_eq!(h.requests("session/cancel"), 1);
    h.release("update-2");
    h.release("release-2");
    tokio::time::timeout(Duration::from_secs(5), async {
        while h.events(&session, "turn.completed").await != 1 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(!h.wire().iter().any(|item| item["overlap"] == true));
    h.stop().await;
}

#[tokio::test]
async fn final_response_without_updates_confirms_started_before_terminal() {
    let h = Harness::new(&[("ACP_NO_UPDATES", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    let events: Vec<String> = sqlx::query_scalar("SELECT event_type FROM events WHERE turn_id=? AND event_type IN ('turn.started','turn.completed') ORDER BY rowid")
        .bind(&turn.turn_id).fetch_all(&h.app.db()).await.unwrap();
    assert_eq!(events, ["turn.started", "turn.completed"]);
    h.stop().await;
}

#[tokio::test]
async fn disconnect_unknown_result_and_shutdown_do_not_invent_terminal_facts() {
    for flags in [vec![("ACP_DROP_PROMPT", "1")], vec![]] {
        let h = Harness::new(&flags).await;
        let session = h.create("first").await.unwrap();
        let turn = h
            .app
            .turn_commands()
            .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
            .await
            .unwrap()
            .unwrap();
        h.release("update-1");
        h.wait_turn(&session, &turn.turn_id, "running").await;
        if !flags.is_empty() {
            h.release("release-1");
        } else {
            h.app.shutdown().notify();
        }
        h.wait_runtime_exited(&session).await;
        let view = h
            .app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap();
        assert!(!view.capabilities.accept_task);
        assert_eq!(view.state, "busy");
        for event in [
            "session.exited",
            "turn.failed",
            "turn.interrupted",
            "turn.completed",
        ] {
            assert_eq!(h.events(&session, event).await, 0);
        }
        assert_eq!(h.requests("session/prompt"), 1);
        h.stop().await;
    }
}

#[tokio::test]
async fn unknown_stop_reason_does_not_guess_a_terminal_state() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "extension_completed".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.wait_turn(&session, &turn.turn_id, "running").await;
    h.release("release-1");
    h.wait_runtime_exited(&session).await;
    assert_eq!(
        h.app
            .queries()
            .get_turn(&session, &turn.turn_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        "running"
    );
    h.stop().await;
}

#[tokio::test]
async fn failed_close_does_not_exit_session_or_terminate_peer() {
    let h = Harness::new(&[("ACP_CLOSE_FAIL", "1")]).await;
    let session = h.create("first").await.unwrap();
    assert!(
        h.app
            .session_commands()
            .terminate_session(&session)
            .await
            .is_err()
    );
    assert_eq!(h.events(&session, "session.exited").await, 0);
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.runtimes[0].state, "running");
    assert!(view.capabilities.accept_task);
    h.stop().await;
}

#[tokio::test]
async fn agent_namespaces_isolate_equal_native_ids_and_duplicate_binding_is_rejected() {
    let h = Harness::new(&[]).await;
    let first = h.create("first").await.unwrap();
    let second = h.create("second").await.unwrap();
    assert_ne!(first, second);
    assert!(h.create("first").await.is_err());
    let bindings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings")
        .fetch_one(&h.app.db())
        .await
        .unwrap();
    assert_eq!(bindings, 2);
    for session in [&first, &second] {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM session_runtimes WHERE session_id=?")
                .bind(session)
                .fetch_one(&h.app.db())
                .await
                .unwrap();
        assert_eq!(count, 1);
    }
    h.stop().await;
}
