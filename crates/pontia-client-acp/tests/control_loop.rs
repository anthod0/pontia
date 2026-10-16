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
    config: AcpConfig,
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
            registry_id: None,
            command: command.into(),
            args: vec![
                format!("{}/tests/fixtures/agent.py", env!("CARGO_MANIFEST_DIR")),
                "argument with spaces; not a shell".into(),
            ],
            env,
        };
        let mut clients = ClientRegistry::default();
        let config = AcpConfig {
            agents: BTreeMap::from([("first".into(), config.clone()), ("second".into(), config)]),
        };
        clients.register(pontia_client_acp::registration(config.clone()));
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
            config,
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
                if self.events(session, "runtime.exited").await > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn wait_interactions(
        &self,
        session: &str,
        count: usize,
    ) -> Vec<pontia_application::interactions::Interaction> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let interactions = self
                    .app
                    .interactions()
                    .list(session, None, 50, None)
                    .await
                    .unwrap()
                    .interactions;
                if interactions.len() == count {
                    return interactions;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("permission registration must not block the reader")
    }

    async fn wait_interaction_state(
        &self,
        session: &str,
        id: &str,
        expected: pontia_application::interactions::InteractionState,
    ) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if self
                    .app
                    .interactions()
                    .get(session, id)
                    .await
                    .unwrap()
                    .state
                    == expected
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn restart(&mut self) {
        self.app.shutdown().notify();
        tokio::time::timeout(Duration::from_secs(5), self.tasks.take().unwrap().join())
            .await
            .unwrap()
            .unwrap();
        let mut clients = ClientRegistry::default();
        clients.register(pontia_client_acp::registration(self.config.clone()));
        self.app = AppState::builder(self.app.db(), self.root.path().into())
            .clients(clients.clone())
            .build();
        self.tasks = Some(
            clients
                .start_services(self.app.clone(), ClientServicePhase::Transport)
                .await
                .unwrap(),
        );
    }

    async fn wait_requests(&self, method: &str, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.requests(method) < count {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn wait_ready(&self, session: &str, count: i64) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if self.events(session, "session.ready").await >= count
                    && self
                        .app
                        .queries()
                        .get_session(session)
                        .await
                        .unwrap()
                        .unwrap()
                        .capabilities
                        .timeline
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Session did not recover: wire={:?}", self.wire()));
    }

    async fn history(
        &self,
        session: &str,
    ) -> std::result::Result<
        pontia_application::TurnTimelinePage,
        pontia_application::TurnTimelineServiceError,
    > {
        pontia_application::TurnTimelineService::new(self.app.event_ingest_service())
            .page(
                session.into(),
                pontia_application::TurnTimelineDirection::Forward,
                None,
                50,
            )
            .await
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
    assert!(view.capabilities.timeline);
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

#[tokio::test]
async fn permission_choices_preserve_all_scopes_and_native_ids_without_blocking_prompt_messages() {
    use pontia_application::interactions::{InteractionError, InteractionIntent, InteractionState};
    let h = Harness::new(&[("ACP_PERMISSION", "1")]).await;
    let session = h.create("first").await.unwrap();
    for (ordinal, selected) in [
        (1, "once:原"),
        (2, "always:原"),
        (3, "reject-once"),
        (4, "reject-always"),
    ] {
        let turn = h
            .app
            .turn_commands()
            .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
            .await
            .unwrap()
            .unwrap();
        h.release(&format!("update-{ordinal}"));
        let records = h.wait_interactions(&session, ordinal * 3).await;
        let records: Vec<_> = records
            .into_iter()
            .filter(|i| i.turn_id.as_deref() == Some(&turn.turn_id))
            .collect();
        assert_eq!(records.len(), 3);
        h.wait_turn(&session, &turn.turn_id, "running").await;
        let expected_id = if ordinal % 2 == 0 {
            json!(format!("permission-{ordinal}"))
        } else {
            json!(700 + ordinal)
        };
        let locator = format!(
            "request:{}",
            expected_id
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| expected_id.to_string())
        );
        let record = records
            .iter()
            .find(|record| {
                record
                    .description
                    .as_ref()
                    .is_some_and(|text| text.ends_with(&locator))
            })
            .unwrap();
        assert_eq!(
            record
                .options
                .iter()
                .map(|o| (o.key.as_str(), &o.intent))
                .collect::<Vec<_>>(),
            vec![
                ("once:原", &InteractionIntent::Positive),
                ("always:原", &InteractionIntent::Positive),
                ("reject-once", &InteractionIntent::Negative),
                ("reject-always", &InteractionIntent::Negative),
            ]
        );
        let public = serde_json::to_string(record).unwrap();
        assert!(!public.contains("do-not-persist"));
        assert!(!public.contains("requestId"));
        assert!(matches!(
            h.app
                .interactions()
                .respond(&h.app, &session, &record.interaction_id, "not-offered")
                .await,
            Err(InteractionError::InvalidOption)
        ));
        let service = h.app.interactions();
        let (first, concurrent) = tokio::join!(
            service.respond(&h.app, &session, &record.interaction_id, selected),
            service.respond(&h.app, &session, &record.interaction_id, selected)
        );
        assert!(first.is_ok() || concurrent.is_ok());
        for result in [&first, &concurrent] {
            if let Err(error) = result {
                assert!(matches!(error, InteractionError::Resolving));
            }
        }
        let repeated = h
            .app
            .interactions()
            .respond(&h.app, &session, &record.interaction_id, "different")
            .await
            .unwrap();
        assert_eq!(repeated.response.unwrap().option_key, selected);
        // Leave the other two requests pending: the final prompt response must still be read.
        h.release(&format!("release-{ordinal}"));
        h.wait_turn(&session, &turn.turn_id, "completed").await;
        for other in records
            .iter()
            .filter(|other| other.interaction_id != record.interaction_id)
        {
            h.wait_interaction_state(&session, &other.interaction_id, InteractionState::Expired)
                .await;
        }
        assert_eq!(
            h.app
                .interactions()
                .get(&session, &record.interaction_id)
                .await
                .unwrap()
                .state,
            InteractionState::Resolved
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let responses: Vec<_> = h
                    .wire()
                    .into_iter()
                    .filter(|r| {
                        r["result"]["outcome"]["outcome"] == "selected"
                            && r["result"]["outcome"]["optionId"] == selected
                    })
                    .collect();
                if responses.len() == 1 {
                    assert_eq!(responses[0]["id"], expected_id);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    assert_eq!(h.requests("session/prompt"), 4);
    h.stop().await;
}

#[tokio::test]
async fn prompt_cancel_replies_cancelled_to_every_pending_request_without_finishing_turn() {
    use pontia_application::interactions::{InteractionError, InteractionState};
    let h = Harness::new(&[("ACP_PERMISSION", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    let records = h.wait_interactions(&session, 3).await;
    h.wait_turn(&session, &turn.turn_id, "running").await;
    h.app
        .turn_commands()
        .interrupt_turn(&session, &turn.turn_id)
        .await
        .unwrap();
    for record in records {
        h.wait_interaction_state(&session, &record.interaction_id, InteractionState::Expired)
            .await;
        assert!(matches!(
            h.app
                .interactions()
                .respond(&h.app, &session, &record.interaction_id, "once:原")
                .await,
            Err(InteractionError::Expired)
        ));
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        while h
            .wire()
            .iter()
            .filter(|r| r["result"]["outcome"]["outcome"] == "cancelled")
            .count()
            != 3
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let replies: Vec<_> = h
        .wire()
        .into_iter()
        .filter(|r| r.get("result").is_some())
        .collect();
    for id in [json!(701), json!("permission-1"), json!("unanswered-1")] {
        assert_eq!(
            replies
                .iter()
                .filter(
                    |r| r["id"] == id && r["result"] == json!({"outcome":{"outcome":"cancelled"}})
                )
                .count(),
            1
        );
    }
    assert_eq!(h.events(&session, "turn.interrupted").await, 0);
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "interrupted").await;
    h.stop().await;
}

#[tokio::test]
async fn native_request_cancellation_expires_only_its_own_interaction() {
    use pontia_application::interactions::{InteractionError, InteractionState};
    let h = Harness::new(&[("ACP_PERMISSION", "1"), ("ACP_NATIVE_CANCEL", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    let records = h.wait_interactions(&session, 3).await;
    h.release("native-cancel-1");
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let page = h
                .app
                .interactions()
                .list(&session, None, 50, None)
                .await
                .unwrap();
            if page
                .interactions
                .iter()
                .filter(|i| i.state == InteractionState::Expired)
                .count()
                == 1
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    for record in records {
        let current = h
            .app
            .interactions()
            .get(&session, &record.interaction_id)
            .await
            .unwrap();
        let result = h
            .app
            .interactions()
            .respond(&h.app, &session, &record.interaction_id, "always:原")
            .await;
        if current.state == InteractionState::Expired {
            assert!(matches!(result, Err(InteractionError::Expired)));
        } else {
            assert_eq!(result.unwrap().state, InteractionState::Resolved);
        }
    }
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    assert!(
        !h.wire()
            .iter()
            .any(|r| r["id"] == "unanswered-1" && r.get("result").is_some())
    );
    h.stop().await;
}

#[tokio::test]
async fn disconnect_and_close_expire_pending_permissions_without_replaying_on_another_peer() {
    use pontia_application::interactions::{InteractionError, InteractionState};
    for close in [false, true] {
        let h = Harness::new(&[("ACP_PERMISSION", "1"), ("ACP_DROP_PROMPT", "1")]).await;
        let session = h.create("first").await.unwrap();
        let turn = h
            .app
            .turn_commands()
            .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
            .await
            .unwrap()
            .unwrap();
        h.release("update-1");
        let records = h.wait_interactions(&session, 3).await;
        h.wait_turn(&session, &turn.turn_id, "running").await;
        if close {
            h.app
                .session_commands()
                .terminate_session(&session)
                .await
                .unwrap();
        } else {
            h.release("release-1");
        }
        h.wait_runtime_exited(&session).await;
        if !close {
            h.wait_ready(&session, 2).await;
        }
        let other = h.create("second").await.unwrap();
        for record in records {
            h.wait_interaction_state(&session, &record.interaction_id, InteractionState::Expired)
                .await;
            assert!(matches!(
                h.app
                    .interactions()
                    .respond(&h.app, &session, &record.interaction_id, "once:原")
                    .await,
                Err(InteractionError::Expired)
            ));
            assert!(matches!(
                h.app
                    .interactions()
                    .respond(&h.app, &other, &record.interaction_id, "once:原")
                    .await,
                Err(InteractionError::NotFound)
            ));
        }
        assert!(!h.wire().iter().any(|r| r.get("result").is_some()));
        assert_eq!(h.events(&session, "turn.completed").await, 0);
        assert_eq!(h.events(&session, "turn.interrupted").await, 0);
        h.stop().await;
    }
}

#[tokio::test]
async fn permission_response_flush_does_not_block_incoming_updates_under_pipe_backpressure() {
    use pontia_application::interactions::InteractionState;
    let h = Harness::new(&[("ACP_PERMISSION", "1"), ("ACP_REPLY_BACKPRESSURE", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    let record = h.wait_interactions(&session, 1).await.remove(0);
    h.wait_turn(&session, &turn.turn_id, "running").await;
    let app = h.app.clone();
    let response_session = session.clone();
    let response_id = record.interaction_id.clone();
    let responding = tokio::spawn(async move {
        app.interactions()
            .respond(&app, &response_session, &response_id, &"x".repeat(262144))
            .await
    });
    h.wait_interaction_state(
        &session,
        &record.interaction_id,
        InteractionState::Resolving,
    )
    .await;
    h.release("backpressure-1");
    let result = tokio::time::timeout(Duration::from_secs(5), responding)
        .await
        .expect("incoming updates must drain while the response waits for stdin capacity")
        .unwrap()
        .unwrap();
    assert_eq!(result.state, InteractionState::Resolved);
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    h.stop().await;
}

#[tokio::test]
async fn load_replay_reads_user_and_agent_history_without_creating_execution_facts() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    assert!(h.history(&session).await.unwrap().items.is_empty());
    let mut turns = Vec::new();
    for ordinal in 1..=2 {
        let turn = h
            .app
            .turn_commands()
            .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
            .await
            .unwrap()
            .unwrap();
        h.release(&format!("update-{ordinal}"));
        h.wait_turn(&session, &turn.turn_id, "running").await;
        // A live read must not issue session/load into the active prompt.
        let loads = h.requests("session/load");
        let live = h.history(&session).await.unwrap();
        assert_eq!(h.requests("session/load"), loads);
        assert!(
            live.items
                .iter()
                .any(|item| item.turn_id.as_deref() == Some(&turn.turn_id)
                    && item.item.role == "assistant")
        );
        h.release(&format!("release-{ordinal}"));
        h.wait_turn(&session, &turn.turn_id, "completed").await;
        turns.push(turn.turn_id);
    }
    let page = h.history(&session).await.unwrap();
    // The existing chat consumer orders untimestamped native items by item ID.
    assert!(
        page.items
            .windows(2)
            .all(|pair| pair[0].item.item_id < pair[1].item.item_id)
    );
    let messages: Vec<_> = page
        .items
        .iter()
        .map(|item| {
            (
                item.turn_id.as_deref().unwrap(),
                item.item.role.as_str(),
                item.item.content_preview.as_str(),
            )
        })
        .collect();
    assert_eq!(
        messages,
        vec![
            (turns[0].as_str(), "user", "end_turn"),
            (turns[0].as_str(), "assistant", "streamed answer"),
            (turns[1].as_str(), "user", "end_turn"),
            (turns[1].as_str(), "assistant", "streamed answer")
        ]
    );
    assert_eq!(h.events(&session, "turn.started").await, 2);
    assert_eq!(h.events(&session, "turn.completed").await, 2);
    let copied: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE payload LIKE '%streamed answer%'")
            .fetch_one(&h.app.db())
            .await
            .unwrap();
    assert_eq!(copied, 0);
    h.stop().await;
}

#[tokio::test]
async fn service_restart_loads_the_bound_agent_identity_and_preserves_runtime_and_history() {
    let mut h = Harness::new(&[]).await;
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
    let before = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    let binding = pontia_application::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    // Selection metadata is not the authority for an already-bound Session.
    sqlx::query("UPDATE sessions SET metadata=? WHERE session_id=?")
        .bind(r#"{"acp_agent_id":"second"}"#)
        .bind(&session)
        .execute(&h.app.db())
        .await
        .unwrap();
    h.restart().await;
    h.wait_ready(&session, 2).await;
    let after = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    let restored = pontia_application::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restored.id, binding.id);
    assert_eq!(restored.client_session_key, binding.client_session_key);
    assert_eq!(after.runtimes.len(), 1);
    assert_eq!(after.runtimes[0].runtime_id, before.runtimes[0].runtime_id);
    #[cfg(target_os = "linux")]
    assert_ne!(
        after.runtimes[0].process_fingerprint,
        before.runtimes[0].process_fingerprint
    );
    assert_eq!(h.requests("session/new"), 1);
    assert_eq!(h.requests("session/prompt"), 1);
    let history = h.history(&session).await.unwrap();
    assert_eq!(history.items.len(), 2);
    assert!(
        history
            .items
            .iter()
            .all(|item| item.turn_id.as_deref() == Some(&turn.turn_id))
    );
    assert_eq!(history.items[1].item.content_preview, "streamed answer");
    assert_eq!(h.events(&session, "turn.completed").await, 1);
    h.stop().await;
}

#[tokio::test]
async fn lost_prompt_reconnect_expires_permissions_without_replay_or_terminal_facts() {
    use pontia_application::interactions::InteractionState;
    let h = Harness::new(&[("ACP_PERMISSION", "1"), ("ACP_DROP_PROMPT", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    let records = h.wait_interactions(&session, 3).await;
    h.wait_turn(&session, &turn.turn_id, "running").await;
    h.release("release-1");
    h.wait_ready(&session, 2).await;
    for record in records {
        h.wait_interaction_state(&session, &record.interaction_id, InteractionState::Expired)
            .await;
    }
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.runtimes.len(), 1);
    assert_eq!(view.state, "busy");
    assert!(!view.capabilities.accept_task);
    assert!(!view.capabilities.interrupt);
    assert!(view.capabilities.timeline);
    assert_eq!(h.history(&session).await.unwrap().items.len(), 2);
    assert_eq!(h.requests("initialize"), 2);
    assert_eq!(h.requests("session/new"), 1);
    assert_eq!(h.requests("session/prompt"), 1);
    assert!(!h.wire().iter().any(|frame| frame.get("result").is_some()));
    for kind in [
        "session.exited",
        "turn.completed",
        "turn.interrupted",
        "turn.failed",
    ] {
        assert_eq!(h.events(&session, kind).await, 0);
    }
    h.stop().await;
}

#[tokio::test]
async fn backpressured_prompt_writes_do_not_report_a_live_history_source_as_unavailable() {
    use pontia_application::client_contract::raw_transcripts::AgentBindingResolveRequest;
    let h = Harness::new(&[("ACP_PAUSE_INPUT_AFTER_LOAD", "1")]).await;
    let session = h.create("first").await.unwrap();
    assert!(
        h.app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .timeline
    );
    let binding = pontia_application::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    let request = AgentBindingResolveRequest {
        id: binding.id,
        session_id: session.clone(),
        client_type: binding.client_type,
        client_session_key: binding.client_session_key,
        client_session_file: None,
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        while !h.root.path().join("input-paused").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let submission = tokio::spawn({
        let app = h.app.clone();
        let session = session.clone();
        async move {
            app.inbox_commands()
                .submit_message(
                    &session,
                    SubmitInboxMessageRequest {
                        input: "x".repeat(2 * 1024 * 1024),
                        metadata: json!({}),
                        delivery_policy: "after_idle".into(),
                        branch_target_turn_id: None,
                    },
                )
                .await
        }
    });
    let clients = h.app.clients();
    let data = clients.data("acp").unwrap();
    // The peer cannot drain its pipe until explicitly released. Probe through
    // the public data contract while the large prompt write is backpressured.
    for _ in 0..64 {
        if let Some(result) = data.probe_timeline(&request) {
            assert!(
                result.is_ok(),
                "a busy lock must not diagnose a failed history source: {result:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert_eq!(h.requests("session/prompt"), 0);
    h.release("resume-input");
    tokio::time::timeout(Duration::from_secs(5), submission)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    h.wait_requests("session/prompt", 1).await;
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(view.capabilities.timeline);
    assert!(view.timeline_unavailable_reason.is_none());
    h.stop().await;
}

#[tokio::test]
async fn failed_history_load_is_not_empty_success_and_can_be_reprobed() {
    let h = Harness::new(&[]).await;
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
    h.release("load-fail");
    assert!(h.history(&session).await.is_err());
    let failed = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(!failed.capabilities.timeline);
    assert!(failed.timeline_unavailable_reason.is_some());
    std::fs::remove_file(h.root.path().join("load-fail")).unwrap();
    let restored = h.history(&session).await.unwrap();
    assert_eq!(restored.items.len(), 2);
    assert_eq!(h.requests("session/new"), 1);
    h.stop().await;
}

#[tokio::test]
async fn empty_history_is_distinct_from_a_failed_or_unassociated_replay() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    assert!(h.history(&session).await.unwrap().items.is_empty());
    for marker in ["load-fail", "orphan-replay"] {
        h.release(marker);
        assert!(h.history(&session).await.is_err());
        let view = h
            .app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap();
        assert!(!view.capabilities.timeline);
        assert!(view.timeline_unavailable_reason.is_some());
        std::fs::remove_file(h.root.path().join(marker)).unwrap();
        assert!(h.history(&session).await.unwrap().items.is_empty());
    }
    assert_eq!(h.events(&session, "turn.started").await, 0);
    h.stop().await;
}

#[tokio::test]
async fn replay_for_another_native_session_is_unavailable_not_empty_history() {
    let h = Harness::new(&[]).await;
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
    h.release("wrong-replay");
    assert!(h.history(&session).await.is_err());
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(!view.capabilities.timeline);
    assert!(view.timeline_unavailable_reason.is_some());
    assert_eq!(h.events(&session, "turn.started").await, 1);
    assert_eq!(h.events(&session, "turn.completed").await, 1);
    h.stop().await;
}

#[tokio::test]
async fn shutdown_during_recovery_cancels_held_load_and_leaves_identity_and_lifecycle_intact() {
    let mut h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    h.release("hold-load");
    h.restart().await;
    h.wait_requests("session/load", 2).await;
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(!view.capabilities.accept_task);
    assert!(!view.capabilities.timeline);
    assert_eq!(view.runtimes.len(), 1);
    assert_eq!(h.requests("session/new"), 1);
    assert_eq!(h.events(&session, "session.exited").await, 0);
    assert_eq!(h.events(&session, "session.ready").await, 1);
    h.stop().await;
}

#[tokio::test]
async fn rechunked_history_preserves_no_output_prompt_boundaries_across_restart() {
    let mut h = Harness::new(&[
        ("ACP_SILENT_FIRST", "1"),
        ("ACP_RECHUNK", "1"),
        ("ACP_STOP_REASON", "end_turn"),
    ])
    .await;
    let session = h.create("first").await.unwrap();
    let mut turns = Vec::new();
    for (ordinal, input) in [(1, "first:雪"), (2, "second:🌊")] {
        let turn = h
            .app
            .turn_commands()
            .create_and_dispatch_turn(&session, input.into(), json!({}))
            .await
            .unwrap()
            .unwrap();
        h.release(&format!("update-{ordinal}"));
        h.release(&format!("release-{ordinal}"));
        h.wait_turn(&session, &turn.turn_id, "completed").await;
        turns.push(turn.turn_id);
    }
    for restart in [false, true] {
        if restart {
            h.restart().await;
            h.wait_ready(&session, 2).await;
        }
        let page = h.history(&session).await.unwrap();
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.items[0].turn_id.as_deref(), Some(turns[0].as_str()));
        assert_eq!(page.items[0].item.content_preview, "first:雪");
        assert_eq!(page.items[1].turn_id.as_deref(), Some(turns[1].as_str()));
        assert_eq!(page.items[1].item.content_preview, "second:🌊");
        assert_eq!(page.items[2].turn_id.as_deref(), Some(turns[1].as_str()));
        assert_eq!(page.items[2].item.content_preview, "streamed answer");
    }
    assert_eq!(h.events(&session, "turn.completed").await, 2);
    h.stop().await;
}

#[tokio::test]
async fn cancelling_a_history_reader_does_not_strand_replay_or_misclassify_late_history() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    h.release("hold-load");
    let app = h.app.clone();
    let read_session = session.clone();
    let read = tokio::spawn(async move {
        pontia_application::TurnTimelineService::new(app.event_ingest_service())
            .page(
                read_session,
                pontia_application::TurnTimelineDirection::Forward,
                None,
                50,
            )
            .await
    });
    h.wait_requests("session/load", 2).await;
    read.abort();
    assert!(read.await.unwrap_err().is_cancelled());
    h.release("release-load");
    h.wait_ready(&session, 1).await;
    assert!(h.history(&session).await.unwrap().items.is_empty());
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
    assert_eq!(h.events(&session, "turn.started").await, 1);
    h.stop().await;
}

#[tokio::test]
async fn a_stalled_peer_does_not_block_other_session_recovery_or_shutdown() {
    let mut h = Harness::new(&[]).await;
    let first = h.create("first").await.unwrap();
    let second = h.create("second").await.unwrap();
    h.config
        .agents
        .get_mut("first")
        .unwrap()
        .env
        .insert("ACP_LOAD_GATE".into(), "first-load".into());
    h.release("first-load");
    h.restart().await;
    h.wait_ready(&second, 2).await;
    let stalled = h.app.queries().get_session(&first).await.unwrap().unwrap();
    assert!(!stalled.capabilities.accept_task);
    assert!(!stalled.capabilities.timeline);
    assert_eq!(h.events(&first, "session.ready").await, 1);
    assert_eq!(h.requests("session/new"), 2);
    h.stop().await;
}

#[tokio::test]
async fn native_history_without_pontia_turns_is_visible_without_inventing_turns() {
    let h = Harness::new(&[]).await;
    let updates = [
        json!({"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"earlier user"}}),
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"earlier answer"}}),
    ];
    std::fs::write(
        h.root.path().join("history.jsonl"),
        updates
            .iter()
            .map(|update| format!("{update}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let session = h.create("first").await.unwrap();
    let page = h.history(&session).await.unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].item.content_preview, "earlier user");
    assert_eq!(page.items[1].item.content_preview, "earlier answer");
    assert!(page.items.iter().all(|item| item.turn_id.is_none()));
    assert!(
        h.app
            .queries()
            .list_turns(&session)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(h.events(&session, "turn.started").await, 0);
    h.stop().await;
}

#[tokio::test]
async fn recovery_rejects_missing_native_session_or_downgraded_capabilities_without_rebinding() {
    for marker in ["load-fail", "no-load"] {
        let mut h = Harness::new(&[]).await;
        let session = h.create("first").await.unwrap();
        let binding = pontia_application::AgentBindingService::new(h.app.db())
            .binding_for_session(&session)
            .await
            .unwrap()
            .unwrap();
        h.release(marker);
        h.restart().await;
        h.wait_requests("initialize", 2).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while h.events(&session, "runtime.exited").await < 2 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let failed = h
            .app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap();
        assert!(!failed.capabilities.accept_task);
        assert!(!failed.capabilities.timeline);
        assert!(failed.timeline_unavailable_reason.is_some());
        assert_eq!(failed.runtimes.len(), 1);
        let after = pontia_application::AgentBindingService::new(h.app.db())
            .binding_for_session(&session)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.client_session_key, binding.client_session_key);
        assert_eq!(after.id, binding.id);
        assert_eq!(h.requests("session/new"), 1);
        assert_eq!(h.events(&session, "session.ready").await, 1);
        assert_eq!(h.events(&session, "session.exited").await, 0);
        h.stop().await;
    }
}

#[tokio::test]
async fn invalid_permission_options_or_native_session_do_not_register_or_approve() {
    for flag in ["ACP_BAD_OPTIONS", "ACP_WRONG_SESSION"] {
        let h = Harness::new(&[("ACP_PERMISSION", "1"), (flag, "1")]).await;
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
        assert!(h.wait_interactions(&session, 0).await.is_empty());
        h.release("release-1");
        h.wait_turn(&session, &turn.turn_id, "completed").await;
        assert!(!h.wire().iter().any(|r| r.get("result").is_some()));
        h.stop().await;
    }
}
