use pontia_agent_control::{
    AgentControl, CreateSessionRequest, SubmitInboxMessageRequest,
    client_contract::ClientServicePhase,
    clients::{ClientRegistry, ClientServiceTasks},
};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::time::Duration;

struct Harness {
    app: AgentControl,
    root: tempfile::TempDir,
    tasks: ClientServiceTasks,
}

#[tokio::test]
async fn dashboard_fork_excludes_the_edited_turn_and_continues_from_inherited_history() {
    let h = Harness::new().await;
    let source = h.create("first prompt").await;
    h.gate(&source, 1, "start");
    h.gate(&source, 1, "finish");
    h.wait_states(&source, &["completed"]).await;
    h.submit(&source, "original second prompt").await;
    h.gate(&source, 2, "start");
    h.gate(&source, 2, "finish");
    let turns = h.wait_states(&source, &["completed", "completed"]).await;

    let outcome = h
        .app
        .session_commands()
        .fork_session(
            &source,
            pontia_agent_control::sessions::ForkSessionRequest {
                operation_id: "edit-second".into(),
                turn_id: turns[1].0.clone(),
            },
        )
        .await
        .unwrap();
    let child = outcome
        .session_id
        .clone()
        .unwrap_or_else(|| panic!("fork failed: {}", serde_json::to_string(&outcome).unwrap()));
    let repeated = h
        .app
        .session_commands()
        .fork_session(
            &source,
            pontia_agent_control::sessions::ForkSessionRequest {
                operation_id: "edit-second".into(),
                turn_id: turns[1].0.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(repeated.session_id.as_deref(), Some(child.as_str()));
    let inherited: Vec<String> =
        sqlx::query_scalar("SELECT input_summary FROM turns WHERE session_id=? ORDER BY rowid")
            .bind(&child)
            .fetch_all(&h.app.db())
            .await
            .unwrap();
    assert_eq!(inherited, ["first prompt"]);
    let lineage: (String, String) = sqlx::query_as(
        "SELECT parent_session_id,forked_from_turn_id FROM session_lineage WHERE child_session_id=?",
    )
    .bind(&child)
    .fetch_one(&h.app.db())
    .await
    .unwrap();
    assert_eq!(lineage, (source, turns[1].0.clone()));

    h.submit(&child, "edited second prompt").await;
    h.gate(&child, 1, "start");
    h.gate(&child, 1, "finish");
    h.wait_states(&child, &["completed", "completed"]).await;
    let inputs: Vec<String> =
        sqlx::query_scalar("SELECT input_summary FROM turns WHERE session_id=? ORDER BY rowid")
            .bind(&child)
            .fetch_all(&h.app.db())
            .await
            .unwrap();
    assert_eq!(inputs, ["first prompt", "edited second prompt"]);
    h.stop().await;
}

#[tokio::test]
async fn dashboard_fork_of_the_first_turn_starts_with_empty_history() {
    let h = Harness::new().await;
    let source = h.create("original prompt").await;
    h.gate(&source, 1, "start");
    h.gate(&source, 1, "finish");
    let turns = h.wait_states(&source, &["completed"]).await;
    let outcome = h
        .app
        .session_commands()
        .fork_session(
            &source,
            pontia_agent_control::sessions::ForkSessionRequest {
                operation_id: "edit-first".into(),
                turn_id: turns[0].0.clone(),
            },
        )
        .await
        .unwrap();
    let child = outcome.session_id.unwrap();
    assert!(h.turns(&child).await.is_empty());

    h.submit(&child, "edited prompt").await;
    h.gate(&child, 1, "start");
    h.gate(&child, 1, "finish");
    h.wait_states(&child, &["completed"]).await;
    let inputs: Vec<String> =
        sqlx::query_scalar("SELECT input_summary FROM turns WHERE session_id=?")
            .bind(&child)
            .fetch_all(&h.app.db())
            .await
            .unwrap();
    assert_eq!(inputs, ["edited prompt"]);
    h.stop().await;
}

#[tokio::test]
async fn replayed_start_without_a_turn_id_returns_the_allocated_claude_turn() {
    use pontia_agent_control::ReportedFact;
    use pontia_core::domain::EventType;

    let h = Harness::new().await;
    let session = h.create("work").await;
    h.gate(&session, 1, "start");
    let turns = h.wait_states(&session, &["running"]).await;
    let events = h.app.event_ingest_service();
    let history = events.list_events(&session).await.unwrap();
    let start = history
        .iter()
        .find(|e| e.event_type == EventType::TurnStarted)
        .unwrap();
    let replay = events
        .report_fact(ReportedFact {
            session_id: session.clone(),
            turn_id: None,
            report_key: start.report_key.clone(),
            fact_type: EventType::TurnStarted,
            data: start.payload.clone(),
        })
        .await
        .unwrap();
    assert!(replay.duplicate);
    assert_eq!(replay.event_id, start.event_id);
    assert_eq!(replay.turn_id.as_deref(), Some(turns[0].0.as_str()));
    assert_eq!(
        events.list_events(&session).await.unwrap().len(),
        history.len()
    );
    h.stop().await;
}

#[tokio::test]
async fn replayed_host_observations_return_original_receipts_after_turn_and_peer_replacement() {
    for reconnect in [false, true] {
        let h = Harness::new().await;
        let session = h.create("capture reports").await;
        h.gate(&session, 1, "start");
        h.gate(&session, 1, "finish");
        h.wait_states(&session, &["completed"]).await;

        for gate in ["compact", "compact-fail"] {
            h.app
                .session_commands()
                .compact_session(&session)
                .await
                .unwrap();
            h.wait_session(&session, "busy").await;
            h.gate(&session, 1, gate);
            h.wait_session(&session, "idle").await;
            std::fs::remove_file(h.root.path().join(format!("{gate}-{session}-1"))).unwrap();
        }
        let usage = h
            .app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .context_usage
            .unwrap();
        assert_eq!(usage.used_tokens, Some(622));
        assert_eq!(usage.confidence, "estimated");
        let h = if reconnect { h.restart().await } else { h };
        h.submit(&session, "replay reports").await;
        let wire = tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let wire = h.wire(&session);
                if wire.iter().any(|f| f["method"] == "late_result") {
                    break wire;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("missing replay receipts: {:?}", h.wire(&session)));
        let replays: Vec<_> = wire.iter().filter(|f| f["method"] == "replayed").collect();
        let mut types = Vec::new();
        for frame in &replays {
            let params = &frame["params"];
            let event = &params["event"];
            let host = params["host_id"].as_str().unwrap();
            let kind = event["kind"].as_str().unwrap();
            let suffix = match kind {
                "started" => format!("{}:start", event["input_id"].as_str().unwrap()),
                "finished" => event["result_id"].as_str().unwrap().to_owned(),
                _ => params["sequence"].as_u64().unwrap().to_string(),
            };
            let key = format!("claude:{host}:{suffix}");
            assert_eq!(params["report_key"], key);
            let original = &frame["original"];
            assert_eq!(original["duplicate"], false);
            let mut expected = original.clone();
            expected["duplicate"] = json!(true);
            assert_eq!(frame["replay"], expected);
            let row: (String, String) =
                sqlx::query_as("SELECT event_type, report_key FROM events WHERE event_id=?")
                    .bind(original["event_id"].as_str().unwrap())
                    .fetch_one(&h.app.db())
                    .await
                    .unwrap();
            assert_eq!(row.1, key);
            types.push(row.0);
        }
        assert_eq!(
            types,
            [
                "session.ready",
                "session.model_updated",
                "session.context_usage_updated",
                "turn.started",
                "turn.completed",
                "session.compaction_started",
                "session.compaction_completed",
                "session.context_usage_updated",
                "session.compaction_started",
                "session.compaction_failed"
            ]
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id=? AND (report_key IS NOT NULL OR event_type='runtime.ready')")
            .bind(&session).fetch_one(&h.app.db()).await.unwrap();
        assert_eq!(count, replays.len() as i64 + 1);
        assert!(
            wire.iter()
                .any(|f| f["method"] == "replay_conflict" && f["code"] == -32009)
        );
        let late = wire.iter().find(|f| f["method"] == "late_result").unwrap();
        assert!(
            wire.iter()
                .any(|f| f["method"] == "result_conflict" && f["code"] == -32009)
        );
        assert_eq!(late["late"], late["changedOutcome"]);
        assert_eq!(late["late"], late["repeated"]);
        assert_eq!(late["late"]["duplicate"], true);
        assert_eq!(late["late"]["event_id"], replays[4]["original"]["event_id"]);
        h.gate(&session, 2, "start");
        h.gate(&session, 2, "finish");
        h.wait_states(&session, &["completed", "completed"]).await;
        h.stop().await;
    }
}

#[tokio::test]
async fn replayed_terminal_report_compares_only_persisted_claude_evidence() {
    use pontia_agent_control::ReportedFact;
    use pontia_core::domain::EventType;
    let h = Harness::new().await;
    let session = h.create("work").await;
    h.gate(&session, 1, "start");
    let turns = h.wait_states(&session, &["running"]).await;
    let runtime = h
        .app
        .queries()
        .session_snapshot(&session)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    let ingest = h.app.event_ingest_service();
    let report = |anchor: &str| ReportedFact {
        session_id: session.clone(),
        turn_id: Some(turns[0].0.clone()),
        report_key: Some("result-1".into()),
        fact_type: EventType::TurnCompleted,
        data: json!({"runtime_id":runtime, "claude_result_id":"result-1", "claude_history_anchor":anchor}),
    };
    let first = ingest.report_fact(report("anchor")).await.unwrap();
    assert!(!first.duplicate);
    for anchor in ["anchor", "different-anchor"] {
        let duplicate = ingest.report_fact(report(anchor)).await.unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.event_id, first.event_id);
    }
    h.stop().await;
}

#[tokio::test]
async fn commands_keep_their_session_environment_after_resume() {
    let h = Harness::new().await;
    let mut sessions = Vec::new();
    for context in ["first", "second"] {
        let mut request: CreateSessionRequest = serde_json::from_value(json!({
            "client_type": "claude-agent", "workspace": h.root.path(),
            "initial_task": {"input": "read-context"}
        }))
        .unwrap();
        request
            .runtime_environment
            .insert("TEST_CONTEXT".into(), context.into());
        let created = h
            .app
            .session_commands()
            .create_session(request)
            .await
            .unwrap();
        sessions.push((created.session_id().unwrap().to_owned(), context));
    }
    for number in 1..=2 {
        for (session, context) in &sessions {
            h.gate(session, number, "start");
            h.gate(session, number, "finish");
            h.wait_states(session, &vec!["completed"; number]).await;
            let values: Vec<_> = h
                .wire(session)
                .into_iter()
                .filter(|frame| frame["method"] == "context")
                .map(|frame| frame["value"].clone())
                .collect();
            assert_eq!(values, vec![json!(context); number]);
            h.app
                .session_commands()
                .terminate_session(session)
                .await
                .unwrap();
        }
        if number == 1 {
            for (session, _) in &sessions {
                h.resume_input(session, "read-context").await;
            }
        }
    }
    h.stop().await;
}

#[tokio::test]
async fn commands_use_their_own_session_environment_before_and_after_resume() {
    let h = Harness::new().await;
    let inherited = std::env::var_os("PONTIA_SESSION_ID");
    let mut sessions = Vec::new();
    for _ in 0..2 {
        let mut request: CreateSessionRequest = serde_json::from_value(json!({
            "client_type":"claude-agent", "workspace":h.root.path(),
            "initial_task":{"input":"read-session-environment"}
        }))
        .unwrap();
        request.runtime_environment.insert(
            "PONTIA_SESSION_ID".into(),
            "wrong-configured-session".into(),
        );
        let created = h
            .app
            .session_commands()
            .create_session(request)
            .await
            .unwrap();
        sessions.push(created.session_id().unwrap().to_owned());
    }
    for number in 1..=2 {
        for session in &sessions {
            h.gate(session, number, "start");
            h.gate(session, number, "finish");
        }
        for session in &sessions {
            h.wait_states(session, &vec!["completed"; number]).await;
            let values: Vec<_> = h
                .wire(session)
                .into_iter()
                .filter(|frame| frame["method"] == "session_environment")
                .map(|frame| frame["value"].clone())
                .collect();
            assert_eq!(values, vec![json!(session); number]);
            h.app
                .session_commands()
                .terminate_session(session)
                .await
                .unwrap();
        }
        if number == 1 {
            for session in &sessions {
                h.resume_input(session, "read-session-environment").await;
            }
        }
    }
    assert_eq!(std::env::var_os("PONTIA_SESSION_ID"), inherited);
    h.stop().await;
}

#[tokio::test]
async fn long_home_uses_the_shared_private_socket_across_exit_and_resume() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::Builder::new()
        .prefix(&"long-pontia-home-".repeat(10))
        .tempdir()
        .unwrap();
    let h = Harness::with_root(root).await;
    let socket = pontia_agent_control::ipc::control_socket_path(h.root.path()).unwrap();
    assert!(socket.as_os_str().len() < 104);
    assert_eq!(
        std::fs::metadata(socket.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let session = h.create("long home").await;
    for number in 1..=2 {
        h.wait_inputs(&session, number).await;
        h.gate(&session, number, "start");
        h.gate(&session, number, "finish");
        h.wait_states(&session, &vec!["completed"; number]).await;
        h.app
            .session_commands()
            .terminate_session(&session)
            .await
            .unwrap();
        assert!(socket.exists());
        if number == 1 {
            h.resume_input(&session, "resumed long home").await;
        }
    }
    h.stop().await;
    assert!(!socket.exists());
}

#[tokio::test]
async fn failed_startup_reports_no_private_host_diagnostics() {
    let h = Harness::new().await;
    std::fs::write(
        h.root.path().join("startup-error.json"),
        json!({"code":"private-secret-path","message":"private-secret-path"}).to_string(),
    )
    .unwrap();
    let request: CreateSessionRequest = serde_json::from_value(json!({"client_type":"claude-agent","workspace":h.root.path(),"initial_task":{"input":"test startup"}})).unwrap();
    let error = h
        .app
        .session_commands()
        .create_session(request)
        .await
        .expect_err("startup must fail")
        .to_string();
    assert!(!error.contains("private-secret-path"));
    h.stop().await;
}

impl Harness {
    async fn new() -> Self {
        Self::with_root(tempfile::tempdir().unwrap()).await
    }
    async fn with_root(root: tempfile::TempDir) -> Self {
        let pool = connect_sqlite("sqlite::memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();
        pontia_agent_control::workspaces::upsert_canonical_workspace(
            &pool,
            root.path().canonicalize().unwrap().to_str().unwrap(),
            None,
        )
        .await
        .unwrap();
        let mut clients = ClientRegistry::default();
        clients.register(pontia_client_claude::registration_with_host(Some(
            format!("{}/tests/fixtures/host.mjs", env!("CARGO_MANIFEST_DIR")).into(),
        )));
        let app = AgentControl::builder(pool, root.path().into())
            .clients(clients.clone())
            .build();
        pontia_agent_control::agent_clients::AgentClientService::new(app.db(), clients.clone())
            .save("claude-agent", "default", true)
            .await
            .unwrap();
        let tasks = clients
            .start_services(app.clone(), ClientServicePhase::Transport)
            .await
            .unwrap();
        Self { app, root, tasks }
    }
    async fn create(&self, input: &str) -> String {
        let request: CreateSessionRequest = serde_json::from_value(json!({"client_type":"claude-agent","workspace":self.root.path(),"initial_task":{"input":input}})).unwrap();
        self.app
            .session_commands()
            .create_session(request)
            .await
            .unwrap()
            .session_id()
            .unwrap()
            .into()
    }
    async fn submit(&self, session: &str, text: &str) {
        self.app
            .inbox_commands()
            .submit_message(
                session,
                SubmitInboxMessageRequest {
                    input: text.into(),
                    metadata: json!({}),
                    delivery_policy: "after_idle".into(),
                },
            )
            .await
            .unwrap();
    }
    fn gate(&self, session: &str, number: usize, kind: &str) {
        std::fs::write(
            self.root.path().join(format!("{kind}-{session}-{number}")),
            "",
        )
        .unwrap();
    }
    fn wire(&self, session: &str) -> Vec<Value> {
        std::fs::read_to_string(self.root.path().join("wire.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|value| value["session"] == session)
            .collect()
    }
    fn host_id(&self, session: &str) -> String {
        self.wire(session)
            .iter()
            .find(|f| f["method"] == "spawn")
            .unwrap()["host"]
            .as_str()
            .unwrap()
            .into()
    }
    async fn assert_terminal_key(&self, session: &str, outcome: &str) {
        let (key, result): (String, String) = sqlx::query_as("SELECT report_key,json_extract(payload,'$.claude_result_id') FROM events WHERE session_id=? AND event_type=?")
            .bind(session).bind(format!("turn.{outcome}")).fetch_one(&self.app.db()).await.unwrap();
        assert_eq!(key, format!("claude:{}:{result}", self.host_id(session)));
    }
    async fn turns(&self, session: &str) -> Vec<(String, String)> {
        sqlx::query_as("SELECT turn_id,state FROM turns WHERE session_id=? ORDER BY rowid")
            .bind(session)
            .fetch_all(&self.app.db())
            .await
            .unwrap()
    }
    async fn wait_states(&self, session: &str, expected: &[&str]) -> Vec<(String, String)> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let turns = self.turns(session).await;
                if turns
                    .iter()
                    .map(|(_, state)| state.as_str())
                    .collect::<Vec<_>>()
                    == expected
                {
                    return turns;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("expected {expected:?}; wire {:?}", self.wire(session)))
    }
    async fn wait_inputs(&self, session: &str, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while self
                .wire(session)
                .iter()
                .filter(|value| value["method"] == "input")
                .count()
                != count
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn wait_live_items(&self, session: &str, turn: &str, count: usize) -> Vec<Value> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(snapshot) = self.app.live_output().snapshot(session, turn)
                    && snapshot.items.len() >= count
                {
                    return snapshot
                        .items
                        .iter()
                        .map(|item| serde_json::to_value(item).unwrap())
                        .collect();
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn interactions(&self, session: &str, expected: &[&str]) -> Vec<Value> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let page = self
                    .app
                    .interactions()
                    .list(session, None, 100, None)
                    .await
                    .unwrap();
                let mut interactions = page.interactions;
                interactions.sort_by(|a, b| a.interaction_id.cmp(&b.interaction_id));
                let values: Vec<Value> = interactions
                    .into_iter()
                    .map(|v| serde_json::to_value(v).unwrap())
                    .collect();
                if values
                    .iter()
                    .map(|v| v["state"].as_str().unwrap())
                    .collect::<Vec<_>>()
                    == expected
                {
                    return values;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn wait_session(&self, session: &str, expected: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while self
                .app
                .queries()
                .get_session(session)
                .await
                .unwrap()
                .unwrap()
                .state
                != expected
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn restart(self) -> Self {
        self.app.shutdown().notify();
        self.tasks.join().await.unwrap();
        let mut clients = ClientRegistry::default();
        clients.register(pontia_client_claude::registration_with_host(Some(
            format!("{}/tests/fixtures/host.mjs", env!("CARGO_MANIFEST_DIR")).into(),
        )));
        let app = AgentControl::builder(self.app.db(), self.root.path().into())
            .clients(clients.clone())
            .build();
        let tasks = clients
            .start_services(app.clone(), ClientServicePhase::Transport)
            .await
            .unwrap();
        Self {
            app,
            root: self.root,
            tasks,
        }
    }

    async fn stop(self) {
        let processes: Vec<String> = sqlx::query_scalar("SELECT process_fingerprint FROM session_runtimes WHERE process_fingerprint IS NOT NULL").fetch_all(&self.app.db()).await.unwrap();
        for process in processes {
            let process: pontia_runtime::ProcessFingerprint =
                serde_json::from_str(&process).unwrap();
            pontia_runtime::GenericRuntimeManager
                .terminate_process(&process)
                .unwrap();
            tokio::time::timeout(Duration::from_secs(3), async {
                while process.observe() != pontia_runtime::ProcessObservation::Exited {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap();
        }
        self.app.shutdown().notify();
        tokio::time::timeout(Duration::from_secs(5), self.tasks.join())
            .await
            .unwrap()
            .unwrap();
    }

    async fn history(
        &self,
        session: &str,
    ) -> Result<
        pontia_agent_control::TurnTreeHistoryPage,
        pontia_agent_control::TurnTimelineServiceError,
    > {
        pontia_agent_control::TurnTimelineService::new(self.app.event_ingest_service())
            .tree_history(session.into(), None, 50)
            .await
    }
    async fn resume_input(&self, session: &str, text: &str) -> Value {
        let message = uuid::Uuid::new_v4().to_string();
        self.app
            .session_commands()
            .resume_for_input(session)
            .await
            .unwrap();
        self.app
            .inbox_commands()
            .submit_message_once(
                &message,
                session,
                SubmitInboxMessageRequest {
                    input: text.into(),
                    delivery_policy: "after_idle".into(),
                    metadata: json!({}),
                },
            )
            .await
            .unwrap()
            .data
    }
}

#[tokio::test]
async fn resume_uses_the_effort_reported_by_the_host() {
    let h = Harness::new().await;
    for (input, observed) in [
        ("observe capped effort", Some("low")),
        ("observe no effort", None),
    ] {
        let request = serde_json::from_value(json!({
            "client_type":"claude-agent", "workspace":h.root.path(),
            "reasoning_effort":"max", "initial_task":{"input":input}
        }))
        .unwrap();
        let created = h
            .app
            .session_commands()
            .create_session(request)
            .await
            .unwrap();
        let session = created.session_id().unwrap();
        h.gate(session, 1, "start");
        h.gate(session, 1, "finish");
        h.wait_states(session, &["completed"]).await;
        let history = h
            .app
            .event_ingest_service()
            .list_events(session)
            .await
            .unwrap();
        let fact = history
            .iter()
            .find(|e| e.event_type == pontia_core::domain::EventType::SessionReasoningEffortUpdated)
            .unwrap();
        assert_eq!(fact.payload.get("reasoning_effort"), Some(&json!(observed)));
        h.app
            .session_commands()
            .terminate_session(session)
            .await
            .unwrap();
        h.resume_input(session, "continue").await;
        let efforts: Vec<_> = h
            .wire(session)
            .into_iter()
            .filter(|v| v["method"] == "spawn")
            .map(|v| v.get("reasoning_effort").cloned())
            .collect();
        assert_eq!(efforts, [Some(json!("max")), observed.map(|v| json!(v))]);
    }
    h.stop().await;
}

#[tokio::test]
async fn running_sessions_change_reasoning_effort_to_the_host_reported_value() {
    let h = Harness::new().await;
    let session = h.create("change effort").await;
    h.wait_inputs(&session, 1).await;
    h.wait_session(&session, "idle").await;
    let capabilities = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap()
        .capabilities;
    assert!(capabilities.reasoning_effort_control);
    h.app
        .session_commands()
        .set_reasoning_effort(
            &session,
            pontia_agent_control::SetSessionReasoningEffortRequest {
                reasoning_effort: "max".into(),
            },
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let view = h
                .app
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap();
            if view.reasoning_effort.as_deref() == Some("low") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(h.wire(&session).iter().any(|frame| {
        frame["method"] == "set_reasoning_effort" && frame["reasoning_effort"] == "max"
    }));
    h.stop().await;
}

#[tokio::test]
async fn exited_sessions_read_and_resume_their_own_history_in_a_shared_workspace() {
    let h = Harness::new().await;
    let a = h.create("alpha history").await;
    let b = h.create("beta history").await;
    for session in [&a, &b] {
        h.gate(session, 1, "start");
        h.gate(session, 1, "finish");
        h.wait_states(session, &["completed"]).await;
        h.app
            .session_commands()
            .terminate_session(session)
            .await
            .unwrap();
    }
    for (session, text) in [(&a, "alpha history"), (&b, "beta history")] {
        let page = h.history(session).await.unwrap();
        assert_eq!(page.groups.len(), 1);
        assert_eq!(
            page.groups[0]
                .items
                .iter()
                .map(|i| i.item.content_preview.as_str())
                .collect::<Vec<_>>(),
            [text, text]
        );
        assert!(
            h.app
                .queries()
                .get_session(session)
                .await
                .unwrap()
                .unwrap()
                .capabilities
                .timeline
        );
        let result = h.resume_input(session, &format!("{text} continued")).await;
        assert_eq!(result["inbox_message"]["state"], "dispatched", "{result}");
        h.wait_inputs(session, 2).await;
        h.gate(session, 2, "start");
        h.gate(session, 2, "finish");
        h.wait_states(session, &["completed", "completed"]).await;
        let spawns: Vec<_> = h
            .wire(session)
            .into_iter()
            .filter(|v| v["method"] == "spawn")
            .collect();
        assert_eq!(spawns.len(), 2);
        assert_eq!(spawns[1]["resume"], spawns[0]["native"]);
        assert_eq!(spawns[1]["native"], spawns[0]["native"]);
        let page = h.history(session).await.unwrap();
        assert_eq!(page.groups.len(), 2, "{page:?}");
        assert_eq!(
            page.groups[1].items[0].item.content_preview,
            format!("{text} continued")
        );
        assert_eq!(page.groups[0].items[0].item.content_preview, text);
    }
    h.stop().await;
}

#[tokio::test]
async fn closing_before_first_turn_reports_unassociated_history_instead_of_empty_success() {
    let h = Harness::new().await;
    let session = h.create("persisted input without a turn").await;
    h.wait_inputs(&session, 1).await;
    h.wait_session(&session, "idle").await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    let result = pontia_agent_control::TurnTimelineService::new(h.app.event_ingest_service())
        .page(
            session.clone(),
            pontia_agent_control::TurnTimelineDirection::Backward,
            None,
            50,
        )
        .await;
    assert!(matches!(
        result,
        Err(pontia_agent_control::TurnTimelineServiceError::CapabilityUnavailable)
    ));
    assert!(h.turns(&session).await.is_empty());
    h.stop().await;
}

#[tokio::test]
async fn relative_config_directory_is_bound_to_the_execution_workspace() {
    if std::env::var_os("PONTIA_TEST_RELATIVE_CONFIG").is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "relative_config_directory_is_bound_to_the_execution_workspace",
                "--nocapture",
            ])
            .env("PONTIA_TEST_RELATIVE_CONFIG", "1")
            .env("CLAUDE_CONFIG_DIR", "native-config")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        return;
    }
    let h = Harness::new().await;
    let session = h.create("relative configuration").await;
    h.gate(&session, 1, "start");
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    let binding = pontia_agent_control::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        binding.metadata["claude_config_dir"],
        h.root.path().join("native-config").to_str().unwrap()
    );
    assert_eq!(
        h.history(&session).await.unwrap().groups[0].items[0]
            .item
            .content_preview,
        "relative configuration"
    );
    let result = h
        .resume_input(&session, "same relative configuration")
        .await;
    assert_eq!(result["inbox_message"]["state"], "dispatched", "{result}");
    h.wait_inputs(&session, 2).await;
    h.stop().await;
}

#[tokio::test]
async fn closed_history_and_resume_survive_a_control_service_restart() {
    let h = Harness::new().await;
    let session = h.create("durable conversation").await;
    h.gate(&session, 1, "start");
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    h.app.shutdown().notify();
    h.tasks.join().await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_claude::registration_with_host(Some(
        format!("{}/tests/fixtures/host.mjs", env!("CARGO_MANIFEST_DIR")).into(),
    )));
    let app = AgentControl::builder(h.app.db(), h.root.path().into())
        .clients(clients.clone())
        .build();
    let tasks = clients
        .start_services(app.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    let h = Harness {
        app,
        root: h.root,
        tasks,
    };
    assert_eq!(
        h.history(&session).await.unwrap().groups[0].items[0]
            .item
            .content_preview,
        "durable conversation"
    );
    let result = h.resume_input(&session, "after service restart").await;
    assert_eq!(result["inbox_message"]["state"], "dispatched", "{result}");
    h.wait_inputs(&session, 2).await;
    h.gate(&session, 2, "start");
    h.gate(&session, 2, "finish");
    h.wait_states(&session, &["completed", "completed"]).await;
    h.stop().await;
}

#[tokio::test]
async fn missing_history_is_an_error_and_never_starts_a_replacement_conversation() {
    let h = Harness::new().await;
    let session = h.create("history to remove").await;
    h.gate(&session, 1, "start");
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    let binding = pontia_agent_control::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.history(&session).await.unwrap().groups[0].items.len(), 2);
    // Remove only this test's native-source fixture under its owned TempDir.
    std::fs::remove_file(
        h.root
            .path()
            .join(format!("history-{}.jsonl", binding.client_session_key)),
    )
    .unwrap();
    assert!(matches!(
        h.history(&session).await,
        Err(pontia_agent_control::TurnTimelineServiceError::SourceUnavailable)
    ));
    assert!(
        h.app
            .session_commands()
            .resume_for_input(&session)
            .await
            .is_err()
    );
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|v| v["method"] == "spawn")
            .count(),
        1
    );
    assert_eq!(h.turns(&session).await.len(), 1);
    assert_eq!(
        pontia_agent_control::AgentBindingService::new(h.app.db())
            .binding_for_session(&session)
            .await
            .unwrap()
            .unwrap()
            .client_session_key,
        binding.client_session_key
    );
    h.stop().await;
}

#[tokio::test]
async fn host_loss_allows_manual_and_input_driven_resume_after_process_exit() {
    for input_driven in [false, true] {
        let h = Harness::new().await;
        let session = h.create("unfinished").await;
        h.gate(&session, 1, "start");
        h.wait_states(&session, &["running"]).await;
        assert!(
            h.app
                .session_commands()
                .resume_session(&session, h.root.path())
                .await
                .is_err()
        );
        h.gate(&session, 1, "lost");
        h.wait_session(&session, "exited").await;
        h.wait_states(&session, &["interrupted"]).await;
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
        if input_driven {
            let result = h.resume_input(&session, "automatic recovery").await;
            assert_eq!(result["inbox_message"]["state"], "dispatched");
        } else {
            h.app
                .session_commands()
                .resume_session(&session, h.root.path())
                .await
                .unwrap();
            h.submit(&session, "manual recovery").await;
        }
        h.wait_inputs(&session, 2).await;
        assert_eq!(
            h.wire(&session)
                .iter()
                .filter(|value| value["method"] == "spawn")
                .count(),
            2
        );
        h.stop().await;
    }
}

#[tokio::test]
async fn host_loss_does_not_resume_when_native_history_is_unreadable() {
    let h = Harness::new().await;
    let session = h.create("unreadable after loss").await;
    h.gate(&session, 1, "start");
    h.wait_states(&session, &["running"]).await;
    let binding = pontia_agent_control::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    h.gate(&session, 1, "lost");
    h.wait_session(&session, "exited").await;
    std::fs::remove_file(
        h.root
            .path()
            .join(format!("history-{}.jsonl", binding.client_session_key)),
    )
    .unwrap();
    assert!(
        h.app
            .session_commands()
            .resume_session(&session, h.root.path())
            .await
            .is_err()
    );
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|value| value["method"] == "spawn")
            .count(),
        1
    );
    h.stop().await;
}

#[tokio::test]
async fn host_exit_after_the_control_socket_closes_still_allows_resume() {
    let h = Harness::new().await;
    let session = h.create("delayed loss").await;
    h.gate(&session, 1, "start");
    h.wait_states(&session, &["running"]).await;
    h.gate(&session, 1, "lost");
    h.wait_session(&session, "exited").await;
    h.wait_states(&session, &["interrupted"]).await;
    let result = h.resume_input(&session, "after delayed exit").await;
    assert_eq!(result["inbox_message"]["state"], "dispatched");
    h.wait_inputs(&session, 2).await;
    h.stop().await;
}

#[tokio::test]
async fn parallel_approvals_are_claimed_once_and_never_release_the_turn_boundary() {
    let h = Harness::new().await;
    let session = h.create("approval-pair").await;
    h.gate(&session, 1, "start");
    h.wait_states(&session, &["running"]).await;
    let approvals = h.interactions(&session, &["pending", "pending"]).await;
    h.wait_session(&session, "waiting_for_interaction").await;
    let a = approvals[0]["interaction_id"].as_str().unwrap();
    let b = approvals[1]["interaction_id"].as_str().unwrap();
    assert!(approvals.iter().all(|p| p["turn_id"].is_null()));
    h.submit(&session, "next").await;
    let service = h.app.interactions();
    let (left, right) = tokio::join!(
        service.respond(&h.app, &session, a, "allow"),
        service.respond(&h.app, &session, a, "deny"),
    );
    assert!(left.is_ok() || right.is_ok());
    h.interactions(&session, &["resolved", "pending"]).await;
    h.wait_session(&session, "waiting_for_interaction").await;
    service
        .respond(&h.app, &session, b, "allow_always")
        .await
        .unwrap();
    let resolved = h.interactions(&session, &["resolved", "resolved"]).await;
    h.wait_session(&session, "busy").await;
    service.respond(&h.app, &session, a, "allow").await.unwrap();
    let choices: Vec<_> = h
        .wire(&session)
        .into_iter()
        .filter(|v| v["method"] == "interaction")
        .collect();
    assert_eq!(choices.len(), 2);
    assert_ne!(choices[0]["interaction_key"], choices[1]["interaction_key"]);
    assert_eq!(choices[0]["option"], resolved[0]["response"]["option_key"]);
    assert_eq!(choices[1]["option"], "allow_always");
    let ingest = h.app.event_ingest_service();
    let interaction_reports: Vec<_> = ingest
        .list_events(&session)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| {
            matches!(
                e.event_type,
                pontia_core::domain::EventType::InteractionBlocked
                    | pontia_core::domain::EventType::InteractionUnblocked
            )
        })
        .collect();
    assert_eq!(interaction_reports.len(), 4);
    let host = h.host_id(&session);
    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT report_key FROM interactions WHERE session_id=? ORDER BY report_key",
    )
    .bind(&session)
    .fetch_all(&h.app.db())
    .await
    .unwrap();
    assert_eq!(
        keys,
        [
            format!("claude:{host}:permission-1-0"),
            format!("claude:{host}:permission-1-1")
        ]
    );
    for event in interaction_reports {
        let id = event.payload["interaction_id"].as_str().unwrap();
        assert_eq!(
            event.report_key,
            Some(format!("claude:{host}:{id}:{}", event.event_type))
        );
        let duplicate = ingest
            .report_fact(pontia_agent_control::ReportedFact {
                session_id: session.clone(),
                turn_id: None,
                fact_type: event.event_type,
                report_key: event.report_key,
                data: event.payload,
            })
            .await
            .unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.event_id, event.event_id);
    }
    assert_eq!(h.turns(&session).await.len(), 1);
    assert_eq!(h.turns(&session).await[0].1, "running");
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|v| v["method"] == "input")
            .count(),
        1
    );
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.wait_inputs(&session, 2).await;
    h.stop().await;
}

#[tokio::test]
async fn waiting_interaction_without_a_turn_can_be_interrupted() {
    let h = Harness::new().await;
    let session = h.create("approval-before-turn").await;
    h.interactions(&session, &["pending"]).await;
    h.wait_session(&session, "waiting_for_interaction").await;
    assert!(h.turns(&session).await.is_empty());
    let outcome = h
        .app
        .turn_commands()
        .interrupt_current_turn(&session)
        .await
        .unwrap();
    assert!(outcome.data["turn"].is_null());
    h.interactions(&session, &["expired"]).await;
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|value| value["method"] == "interrupt")
            .count(),
        1
    );
    h.stop().await;
}

#[tokio::test]
async fn rejects_oversized_host_cards_and_registers_the_next_valid_request() {
    let h = Harness::new().await;
    let session = h.create("approval-limits").await;
    h.gate(&session, 1, "start");
    let cards = h.interactions(&session, &["pending"]).await;
    assert_eq!(cards[0]["title"], "Allow Write?");
    assert_eq!(cards[0]["description"], "fixture.txt");
    assert_eq!(cards[0]["options"][1]["key"], "allow_always");
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|v| v["method"] == "invalid_card" && v["code"] == -32602)
            .count(),
        4
    );
    h.stop().await;
}

#[tokio::test]
async fn conflicting_authorization_expires_the_original_and_releases_the_wait() {
    let h = Harness::new().await;
    let session = h.create("approval-conflict").await;
    h.gate(&session, 1, "start");
    let approvals = h.interactions(&session, &["expired"]).await;
    let id = approvals[0]["interaction_id"].as_str().unwrap();
    h.wait_session(&session, "busy").await;
    assert!(matches!(
        h.app
            .interactions()
            .respond(&h.app, &session, id, "allow")
            .await,
        Err(pontia_agent_control::interactions::InteractionError::Expired)
    ));
    let wire = h.wire(&session);
    assert_eq!(
        wire.iter()
            .filter(|v| v["method"] == "permission_conflict" && v["code"] == -32009)
            .count(),
        1
    );
    assert!(!wire.iter().any(|v| v["method"] == "interaction"));
    h.stop().await;
}

#[tokio::test]
async fn cancellation_disconnect_and_host_loss_reject_stale_choices_without_turn_results() {
    for action in ["cancel", "lost", "interrupt", "lost-during-choice"] {
        let h = Harness::new().await;
        let session = h
            .create(if action == "lost-during-choice" {
                "approval-lost"
            } else {
                "approval"
            })
            .await;
        h.gate(&session, 1, "start");
        let turn = h.wait_states(&session, &["running"]).await[0].0.clone();
        let approvals = h.interactions(&session, &["pending"]).await;
        let id = approvals[0]["interaction_id"].as_str().unwrap();
        if action == "interrupt" {
            h.app
                .turn_commands()
                .interrupt_turn(&session, &turn)
                .await
                .unwrap();
        } else if action == "lost-during-choice" {
            assert!(
                h.app
                    .interactions()
                    .respond(&h.app, &session, id, "allow")
                    .await
                    .is_err()
            );
        } else {
            h.gate(&session, 1, action);
        }
        h.interactions(&session, &["expired"]).await;
        assert!(
            h.app
                .interactions()
                .respond(&h.app, &session, id, "allow")
                .await
                .is_err()
        );
        if matches!(action, "lost" | "lost-during-choice") {
            h.wait_states(&session, &["interrupted"]).await;
        }
        assert_eq!(
            h.turns(&session).await[0].1,
            if matches!(action, "lost" | "lost-during-choice") {
                "interrupted"
            } else {
                "running"
            }
        );
        assert_eq!(
            h.wire(&session)
                .iter()
                .filter(|v| v["method"] == "interaction")
                .count(),
            usize::from(action == "lost-during-choice")
        );
        h.stop().await;
    }
}

#[tokio::test]
async fn background_approval_does_not_block_following_foreground_input() {
    let h = Harness::new().await;
    let session = h.create("approval-background").await;
    h.gate(&session, 1, "start");
    let approvals = h.interactions(&session, &["pending"]).await;
    h.wait_session(&session, "busy").await;
    h.submit(&session, "next").await;
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.wait_inputs(&session, 2).await;
    h.gate(&session, 2, "start");
    h.wait_states(&session, &["completed", "running"]).await;
    h.interactions(&session, &["pending"]).await;
    h.wait_session(&session, "busy").await;
    h.app
        .interactions()
        .respond(
            &h.app,
            &session,
            approvals[0]["interaction_id"].as_str().unwrap(),
            "deny",
        )
        .await
        .unwrap();
    assert_eq!(h.turns(&session).await[1].1, "running");
    h.stop().await;
}

#[tokio::test]
async fn approvals_cannot_be_resolved_from_another_session_in_the_same_workspace() {
    let h = Harness::new().await;
    let a = h.create("approval").await;
    let b = h.create("approval").await;
    h.gate(&a, 1, "start");
    h.gate(&b, 1, "start");
    let a_asks = h.interactions(&a, &["pending"]).await;
    h.interactions(&b, &["pending"]).await;
    let id = a_asks[0]["interaction_id"].as_str().unwrap();
    assert!(
        h.app
            .interactions()
            .respond(&h.app, &b, id, "allow")
            .await
            .is_err()
    );
    h.app
        .interactions()
        .respond(&h.app, &a, id, "allow")
        .await
        .unwrap();
    h.interactions(&b, &["pending"]).await;
    assert!(!h.wire(&b).iter().any(|v| v["method"] == "interaction"));
    h.app
        .session_commands()
        .terminate_session(&b)
        .await
        .unwrap();
    h.interactions(&b, &["expired"]).await;
    h.stop().await;
}

#[tokio::test]
async fn initial_input_binds_identity_once_and_inbox_waits_for_authoritative_end() {
    let h = Harness::new().await;
    let session = h.create("first").await;
    h.wait_inputs(&session, 1).await;
    assert!(h.turns(&session).await.is_empty(), "receipt is not start");
    h.submit(&session, "second").await;
    h.gate(&session, 1, "start");
    let first = h.wait_states(&session, &["running"]).await;
    let binding = pontia_agent_control::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(!binding.client_session_key.is_empty());
    assert_eq!(binding.launch_cwd, h.root.path().to_str().unwrap());
    let inbox: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT state,turn_id FROM inbox_messages WHERE session_id=? ORDER BY rowid",
    )
    .bind(&session)
    .fetch_all(&h.app.db())
    .await
    .unwrap();
    assert_eq!(inbox[0].1.as_deref(), Some(first[0].0.as_str()));
    assert_eq!(inbox[1], ("pending".into(), None));
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|v| v["method"] == "input")
            .count(),
        1
    );
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.wait_inputs(&session, 2).await;
    h.gate(&session, 2, "start");
    h.gate(&session, 2, "finish");
    h.wait_states(&session, &["completed", "completed"]).await;
    assert_eq!(
        pontia_agent_control::AgentBindingService::new(h.app.db())
            .binding_for_session(&session)
            .await
            .unwrap()
            .unwrap()
            .client_session_key,
        binding.client_session_key
    );
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
    h.stop().await;
}

#[tokio::test]
async fn interruption_receipt_waits_for_result_and_normal_completion_can_win() {
    for (text, expected) in [
        ("interrupt", "interrupted"),
        ("completion-wins", "completed"),
    ] {
        let h = Harness::new().await;
        let session = h.create(text).await;
        h.gate(&session, 1, "start");
        let turn = h.wait_states(&session, &["running"]).await[0].0.clone();
        h.app
            .turn_commands()
            .interrupt_turn(&session, &turn)
            .await
            .unwrap();
        h.submit(&session, "next").await;
        assert_eq!(h.turns(&session).await[0].1, "running");
        assert_eq!(
            h.wire(&session)
                .iter()
                .filter(|v| v["method"] == "input")
                .count(),
            1
        );
        h.gate(&session, 1, "finish");
        h.wait_states(&session, &[expected]).await;
        h.assert_terminal_key(&session, expected).await;
        h.wait_inputs(&session, 2).await;
        h.stop().await;
    }
}

#[tokio::test]
async fn conditional_exit_waits_for_host_and_process_closure() {
    let h = Harness::new().await;
    let session = h.create("held").await;
    h.wait_inputs(&session, 1).await;
    let commands = h.app.session_commands();
    assert!(!commands.exit_if_idle(&session).await.unwrap());
    h.gate(&session, 1, "start");
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    // Submit directly so the SDK accepts input before its started fact creates a Turn.
    h.app
        .turn_commands()
        .start_turn(&session, "follow up".into(), json!({}))
        .await
        .unwrap();
    assert_eq!(
        h.app
            .queries()
            .session_snapshot(&session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle"
    );
    assert!(!commands.exit_if_idle(&session).await.unwrap());
    assert!(
        h.wire(&session)
            .iter()
            .any(|request| request["method"] == "is_idle")
    );
    h.gate(&session, 2, "start");
    h.gate(&session, 2, "finish");
    h.wait_states(&session, &["completed", "completed"]).await;
    assert!(commands.exit_if_idle(&session).await.unwrap());
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
    assert!(commands.exit_if_idle(&session).await.unwrap());
    let original: (String, String) = sqlx::query_as(
        "SELECT event_id,payload FROM events WHERE session_id=? AND event_type='session.exited'",
    )
    .bind(&session)
    .fetch_one(&h.app.db())
    .await
    .unwrap();
    let duplicate = h
        .app
        .event_ingest_service()
        .report_fact(pontia_agent_control::ReportedFact {
            session_id: session.clone(),
            turn_id: None,
            report_key: None,
            fact_type: pontia_core::domain::EventType::SessionExited,
            data: serde_json::from_str(&original.1).unwrap(),
        })
        .await
        .unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.event_id, original.0);
    h.stop().await;
}

#[tokio::test]
async fn explicit_sdk_close_interrupts_active_turn_without_fabricating_result() {
    let h = Harness::new().await;
    let session = h.create("held").await;
    h.gate(&session, 1, "start");
    h.wait_states(&session, &["running"]).await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    h.wait_states(&session, &["interrupted"]).await;
    let history = h.history(&session).await.unwrap();
    assert_eq!(history.groups[0].items[0].item.content_preview, "held");
    assert!(history.groups[0].history_issue.is_none());
    let runtime: String =
        sqlx::query_scalar("SELECT state FROM session_runtimes WHERE session_id=?")
            .bind(&session)
            .fetch_one(&h.app.db())
            .await
            .unwrap();
    assert_eq!(runtime, "exited");
    h.stop().await;
}

#[tokio::test]
async fn lost_input_receipt_is_unknown_and_never_replayed() {
    let h = Harness::new().await;
    let session = h.create("unknown").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let state: String =
                sqlx::query_scalar("SELECT state FROM inbox_messages WHERE session_id=?")
                    .bind(&session)
                    .fetch_one(&h.app.db())
                    .await
                    .unwrap();
            if state == "unknown" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    h.app.inbox_commands().notify_available(&session);
    h.app.inbox_commands().drain_inbox(&session).await.unwrap();
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|v| v["method"] == "input")
            .count(),
        1
    );
    assert!(h.turns(&session).await.is_empty());
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
async fn same_workspace_sessions_keep_identity_input_and_live_output_separate() {
    let h = Harness::new().await;
    let a = h.create("alpha").await;
    let b = h.create("beta").await;
    for session in [&a, &b] {
        h.gate(session, 1, "start");
    }
    let ta = h.wait_states(&a, &["running"]).await[0].0.clone();
    let tb = h.wait_states(&b, &["running"]).await[0].0.clone();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if h.app
                .live_output()
                .snapshot(&a, &ta)
                .is_some_and(|s| !s.items.is_empty())
                && h.app
                    .live_output()
                    .snapshot(&b, &tb)
                    .is_some_and(|s| !s.items.is_empty())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    for (session, turn, text) in [(&a, &ta, "alpha"), (&b, &tb, "beta")] {
        let snapshot = h.app.live_output().snapshot(session, turn).unwrap();
        assert!(
            matches!(&snapshot.items[..], [pontia_agent_control::LiveOutputItem::AssistantText { text: value, .. }] if value == text)
        );
    }
    let bindings = pontia_agent_control::AgentBindingService::new(h.app.db());
    assert_ne!(
        bindings
            .binding_for_session(&a)
            .await
            .unwrap()
            .unwrap()
            .client_session_key,
        bindings
            .binding_for_session(&b)
            .await
            .unwrap()
            .unwrap()
            .client_session_key
    );
    h.app
        .session_commands()
        .terminate_session(&a)
        .await
        .unwrap();
    assert_eq!(h.turns(&b).await[0].1, "running");
    h.stop().await;
}

#[tokio::test]
async fn reasoning_deltas_share_live_output_with_text_and_tools() {
    use pontia_agent_control::{LiveOutputItem, LiveToolOutcome, LiveToolPhase};

    let h = Harness::new().await;
    let session = h.create("reasoning-tool").await;
    h.gate(&session, 1, "start");
    let turn = h.wait_states(&session, &["running"]).await[0].0.clone();
    let snapshot = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(snapshot) = h.app.live_output().snapshot(&session, &turn)
                && matches!(
                    snapshot.items.get(2),
                    Some(LiveOutputItem::ToolCall {
                        status: Some(status),
                        result: Some(_),
                        ..
                    }) if status.phase == LiveToolPhase::Ended
                        && status.outcome == Some(LiveToolOutcome::Failed)
                )
            {
                break snapshot;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        &snapshot.items[0],
        LiveOutputItem::Reasoning { text, truncated: false, .. } if text == "Consider options"
    ));
    assert!(matches!(
        &snapshot.items[1],
        LiveOutputItem::AssistantText { text, .. } if text == "reasoning-tool"
    ));
    assert!(matches!(
        &snapshot.items[2],
        LiveOutputItem::ToolCall { status: Some(status), result: Some(_), .. }
            if status.phase == LiveToolPhase::Ended
                && status.outcome == Some(LiveToolOutcome::Failed)
    ));
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.stop().await;
}

#[tokio::test]
async fn live_read_tool_uses_the_standard_managed_card() {
    let h = Harness::new().await;
    let session = h.create("managed-tools").await;
    h.gate(&session, 1, "start");
    let turn = h.wait_states(&session, &["running"]).await[0].0.clone();
    let items = h.wait_live_items(&session, &turn, 5).await;
    let read = items
        .iter()
        .find(|item| item["call_id"] == "read-call")
        .unwrap();
    assert_eq!(
        read["managed_tool_use"],
        json!({
            "tool_name":"Read",
            "input":{"type":"read","path":"src/lib.rs","start_line":2,"end_line":5}
        })
    );
    h.stop().await;
}

#[tokio::test]
async fn live_unknown_tool_keeps_the_generic_card() {
    let h = Harness::new().await;
    let session = h.create("managed-tools").await;
    h.gate(&session, 1, "start");
    let turn = h.wait_states(&session, &["running"]).await[0].0.clone();
    let items = h.wait_live_items(&session, &turn, 5).await;
    let glob = items
        .iter()
        .find(|item| item["call_id"] == "glob-call")
        .unwrap();
    assert!(glob.get("managed_tool_use").is_none());
    assert_eq!(glob["arguments"], json!({"pattern":"**/*.rs"}));
    h.stop().await;
}

#[tokio::test]
async fn live_tool_progress_marks_the_call_running() {
    let h = Harness::new().await;
    let session = h.create("managed-tools").await;
    h.gate(&session, 1, "start");
    let turn = h.wait_states(&session, &["running"]).await[0].0.clone();
    let bash = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(snapshot) = h.app.live_output().snapshot(&session, &turn)
                && let Some(item) = snapshot
                    .items
                    .iter()
                    .find(|item| serde_json::to_value(item).unwrap()["call_id"] == "bash-call")
            {
                let item = serde_json::to_value(item).unwrap();
                if item["status"]["phase"] == "running" {
                    break item;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(bash["status"], json!({"phase":"running"}));
    h.stop().await;
}

#[tokio::test]
async fn history_read_tool_uses_the_standard_managed_card() {
    let h = Harness::new().await;
    let session = h.create("managed-tools").await;
    h.gate(&session, 1, "start");
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    let page = h.history(&session).await.unwrap();
    let read = page.groups[0]
        .items
        .iter()
        .find(|item| item.item.call_id.as_deref() == Some("read-call"))
        .unwrap();
    assert_eq!(
        serde_json::to_value(&read.item.managed_tool_use).unwrap(),
        json!({
            "tool_name":"Read",
            "input":{"type":"read","path":"src/lib.rs","start_line":2,"end_line":5}
        })
    );
    h.stop().await;
}

#[tokio::test]
async fn history_unknown_tool_keeps_the_generic_card() {
    let h = Harness::new().await;
    let session = h.create("managed-tools").await;
    h.gate(&session, 1, "start");
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    let page = h.history(&session).await.unwrap();
    let glob = page.groups[0]
        .items
        .iter()
        .find(|item| item.item.call_id.as_deref() == Some("glob-call"))
        .unwrap();
    assert!(glob.item.managed_tool_use.is_none());
    assert_eq!(glob.item.content_preview, r#"{"pattern":"**/*.rs"}"#);
    h.stop().await;
}

#[tokio::test]
async fn failed_tool_does_not_fail_turn_and_explicit_failure_does() {
    for (text, expected) in [("tool", "completed"), ("fail", "failed")] {
        let h = Harness::new().await;
        let session = h.create(text).await;
        h.gate(&session, 1, "start");
        h.wait_states(&session, &["running"]).await;
        h.gate(&session, 1, "finish");
        h.wait_states(&session, &[expected]).await;
        h.assert_terminal_key(&session, expected).await;
        h.stop().await;
    }
}

#[tokio::test]
async fn service_shutdown_is_not_a_session_or_turn_result() {
    let h = Harness::new().await;
    let session = h.create("held").await;
    h.gate(&session, 1, "start");
    h.wait_states(&session, &["running"]).await;
    let db = h.app.db();
    h.stop().await;
    let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
        .bind(&session)
        .fetch_one(&db)
        .await
        .unwrap();
    let turn: String = sqlx::query_scalar("SELECT state FROM turns WHERE session_id=?")
        .bind(&session)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(state, "busy");
    assert_eq!(turn, "running");
}

#[tokio::test]
async fn unsupported_result_disables_input_without_inventing_a_terminal_turn() {
    let h = Harness::new().await;
    let session = h.create("unsupported").await;
    h.gate(&session, 1, "start");
    h.wait_states(&session, &["running"]).await;
    h.gate(&session, 1, "finish");
    tokio::time::timeout(Duration::from_secs(5), async {
        while h
            .app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .accept_task
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(h.turns(&session).await[0].1, "running");
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    h.wait_states(&session, &["interrupted"]).await;
    h.stop().await;
}

#[tokio::test]
async fn standalone_session_instructions_are_fixed_before_input_and_survive_resume() {
    let h = Harness::new().await;
    let request =
        serde_json::from_value(json!({"client_type":"claude-agent","workspace":h.root.path(),"system_instructions":"caller-marker","include_standalone_session_instructions":true,"mode":"restricted"}))
            .unwrap();
    let created = h
        .app
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = created.session_id().unwrap();
    sqlx::query("DROP TABLE agent_profiles")
        .execute(&h.app.db())
        .await
        .unwrap();
    h.submit(session, "first input").await;
    h.gate(session, 1, "start");
    h.gate(session, 1, "finish");
    h.wait_states(session, &["completed"]).await;
    h.app
        .session_commands()
        .terminate_session(session)
        .await
        .unwrap();
    h.resume_input(session, "resumed input").await;
    h.gate(session, 2, "start");
    h.gate(session, 2, "finish");
    h.wait_states(session, &["completed", "completed"]).await;
    let prompts: Vec<_> = h
        .wire(session)
        .into_iter()
        .filter(|frame| frame["method"] == "spawn")
        .map(|frame| frame["system_prompt"].clone())
        .collect();
    let expected = format!(
        "{}\n\ncaller-marker",
        pontia_agent_control::standalone_session_instructions()
    );
    assert_eq!(prompts, vec![json!(expected), json!(expected)]);
    let modes: Vec<_> = h
        .wire(session)
        .into_iter()
        .filter(|frame| frame["method"] == "spawn")
        .map(|frame| frame["mode"].clone())
        .collect();
    assert_eq!(modes, vec![json!("restricted"); 2]);
    h.stop().await;
}

#[tokio::test]
async fn session_instructions_are_validated_fixed_and_preserved_across_resume() {
    let h = Harness::new().await;
    for system in ["", " "] {
        let request = serde_json::from_value(json!({"client_type":"claude-agent", "workspace":h.root.path(), "system_instructions":system})).unwrap();
        assert!(matches!(
            h.app.session_commands().create_session(request).await,
            Err(pontia_core::Error::Domain(_))
        ));
    }
    let request = serde_json::from_value(json!({
        "client_type": "claude-agent",
        "workspace": h.root.path(),
        "system_instructions": "explicit-marker\n\n{{name}}",
        "include_standalone_session_instructions": true,
    }))
    .unwrap();
    let created = h
        .app
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = created.session_id().unwrap();
    sqlx::query("DROP TABLE agent_profiles")
        .execute(&h.app.db())
        .await
        .unwrap();
    h.submit(session, "first input").await;
    h.gate(session, 1, "start");
    h.gate(session, 1, "finish");
    h.wait_states(session, &["completed"]).await;
    h.app
        .session_commands()
        .terminate_session(session)
        .await
        .unwrap();
    h.resume_input(session, "resumed input").await;
    h.gate(session, 2, "start");
    h.gate(session, 2, "finish");
    h.wait_states(session, &["completed", "completed"]).await;
    let prompts: Vec<_> = h
        .wire(session)
        .into_iter()
        .filter(|frame| frame["method"] == "spawn")
        .map(|frame| frame["system_prompt"].clone())
        .collect();
    let expected = format!(
        "{}\n\nexplicit-marker\n\n{{{{name}}}}",
        pontia_agent_control::standalone_session_instructions()
    );
    assert_eq!(prompts, vec![json!(expected), json!(expected)]);
    h.stop().await;
}

#[tokio::test]
async fn steering_uses_the_active_turn_while_after_idle_waits() {
    let h = Harness::new().await;
    let session = h.create("original").await;
    h.gate(&session, 1, "start");
    let turns = h.wait_states(&session, &["running"]).await;
    h.submit(&session, "queued").await;
    let steered = h
        .app
        .inbox_commands()
        .submit_message(
            &session,
            SubmitInboxMessageRequest {
                input: "correction".into(),
                metadata: json!({}),
                delivery_policy: "steer".into(),
            },
        )
        .await
        .unwrap();
    let steered = steered.data;
    assert_eq!(steered["inbox_message"]["state"], "dispatched");
    assert_eq!(steered["inbox_message"]["turn_id"], turns[0].0);
    let wire = h.wire(&session);
    let original = wire.iter().find(|v| v["method"] == "input").unwrap();
    let steering = wire.iter().find(|v| v["method"] == "steer").unwrap();
    assert_eq!(steering["target_input_id"], original["input_id"]);
    assert_ne!(steering["input_id"], original["input_id"]);
    assert_eq!(steering["text"], "correction");
    assert_eq!(wire.iter().filter(|v| v["method"] == "input").count(), 1);
    let mut items = Vec::new();
    for _ in 0..200 {
        items = h
            .app
            .live_output()
            .snapshot(&session, &turns[0].0)
            .map_or_else(Vec::new, |snapshot| {
                serde_json::to_value(snapshot.items)
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .clone()
            });
        if items.len() >= 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        items
            .iter()
            .map(|item| item["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["assistant_text", "user_input", "assistant_text"]
    );
    assert_eq!(items[1]["text"], "correction");
    assert_eq!(h.turns(&session).await, turns);
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.app.inbox_commands().drain_inbox(&session).await.unwrap();
    h.wait_inputs(&session, 2).await;
    h.gate(&session, 2, "start");
    h.gate(&session, 2, "finish");
    h.wait_states(&session, &["completed", "completed"]).await;
    h.stop().await;
}

#[tokio::test]
async fn yolo_mode_reaches_the_sdk_host() {
    let h = Harness::new().await;
    let request = serde_json::from_value(json!({
        "client_type": "claude-agent",
        "workspace": h.root.path(),
        "mode": "yolo",
    }))
    .unwrap();
    let created = h
        .app
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = created.session_id().unwrap();
    h.submit(session, "yolo task").await;
    h.gate(session, 1, "start");
    h.gate(session, 1, "finish");
    h.wait_states(session, &["completed"]).await;
    let spawn = h
        .wire(session)
        .into_iter()
        .find(|frame| frame["method"] == "spawn")
        .expect("spawn frame");
    assert_eq!(spawn["mode"], "yolo");
    h.stop().await;
}

#[tokio::test]
async fn unopened_history_keeps_both_terminal_turns_when_claude_compacts() {
    use pontia_agent_control::{TurnTimelineDirection, TurnTimelineService};

    let h = Harness::new().await;
    let session = h.create("first input").await;
    h.gate(&session, 1, "start");
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.submit(&session, "second input").await;
    h.gate(&session, 2, "start");
    h.gate(&session, 2, "finish");
    let turns = h.wait_states(&session, &["completed", "completed"]).await;

    h.app
        .session_commands()
        .compact_session(&session)
        .await
        .unwrap();
    let binding = pontia_agent_control::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    let path = h
        .root
        .path()
        .join(format!("history-{}.jsonl", binding.client_session_key));
    let summary = json!({"session_id":binding.client_session_key,"uuid":"summary","type":"user","isCompactSummary":true});
    let history = std::fs::read_to_string(&path).unwrap();
    // The SDK now exposes only its summary and the retained second Turn.
    std::fs::write(
        &path,
        format!(
            "{summary}\n{}\n",
            history.lines().skip(2).collect::<Vec<_>>().join("\n")
        ),
    )
    .unwrap();
    h.gate(&session, 1, "compact");
    h.wait_session(&session, "idle").await;

    let page = TurnTimelineService::new(h.app.event_ingest_service())
        .page(session.clone(), TurnTimelineDirection::Backward, None, 20)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    for entry in page.items {
        assert_eq!(entry.turn_id.as_deref(), Some(turns[1].0.as_str()));
        assert_eq!(entry.item.content_preview, "second input");
    }

    let cache = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(h.root.path().join("cache/session-timeline.db"))
            .read_only(true),
    )
    .await
    .unwrap();
    for ((turn, _), expected) in turns.iter().zip(["first input", "second input"]) {
        let (encoding, payload): (i64, Vec<u8>) = sqlx::query_as(
            "SELECT encoding,payload FROM cached_turns WHERE session_id=? AND source_key='native' AND generation=0 AND turn_id=?"
        ).bind(&session).bind(turn).fetch_one(&cache).await.unwrap();
        let bytes = match encoding {
            0 => payload,
            1 => zstd::decode_all(&payload[..]).unwrap(),
            _ => panic!("unexpected cached range encoding {encoding}"),
        };
        let items: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(items.len(), 2);
        for (item, role) in items.iter().zip(["user", "assistant"]) {
            assert_eq!(item["role"], role);
            assert_eq!(item["content_preview"], expected);
        }
    }
    let event: (String, i64) = sqlx::query_as("SELECT event_id,unixepoch(occurred_at) FROM events WHERE session_id=? AND event_type='session.compaction_completed'")
        .bind(&session).fetch_one(&h.app.db()).await.unwrap();
    let markers: Vec<(String, i64)> =
        sqlx::query_as("SELECT event_id,occurred_at FROM cached_compactions WHERE session_id=?")
            .bind(&session)
            .fetch_all(&cache)
            .await
            .unwrap();
    assert_eq!(markers, [event]);
    h.stop().await;
}

#[tokio::test]
async fn compacted_sdk_history_keeps_readable_turns_and_resumes_without_private_summaries() {
    use pontia_agent_control::{TurnTimelineDirection, TurnTimelineService};
    let h = Harness::new().await;
    let session = h.create("old input").await;
    h.gate(&session, 1, "start");
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    h.submit(&session, "retained input").await;
    h.gate(&session, 2, "start");
    h.gate(&session, 2, "finish");
    let turns = h.wait_states(&session, &["completed", "completed"]).await;
    let binding = pontia_agent_control::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    let path = h
        .root
        .path()
        .join(format!("history-{}.jsonl", binding.client_session_key));
    let source = std::fs::read_to_string(&path).unwrap();
    let rows: Vec<Value> = source
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let write = |messages: &[Value]| {
        std::fs::write(
            &path,
            messages
                .iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>(),
        )
        .unwrap()
    };
    let summary = json!({"session_id":binding.client_session_key,"uuid":"compact-summary","type":"user","isCompactSummary":true,"is_meta":true,"message":{"role":"user","content":"private summary"}});
    let command = json!({"session_id":binding.client_session_key,"uuid":"local-command","type":"user","isCompletedLocalCommand":true,"message":{"role":"user","content":"local command result"}});
    let service = TurnTimelineService::new(h.app.event_ingest_service());
    let page = || service.page(session.clone(), TurnTimelineDirection::Backward, None, 20);
    let original = page().await.unwrap();
    assert_eq!(original.items.len(), 4);
    let reads = || std::fs::read_to_string(h.root.path().join("history-reads")).unwrap();
    let first_reads = reads();
    assert_eq!(page().await.unwrap().items, original.items);
    assert_eq!(reads(), first_reads);
    let omit_size = h.root.path().join("omit-file-size");
    std::fs::write(&omit_size, "").unwrap();
    assert_eq!(page().await.unwrap().items, original.items);
    assert!(reads().len() > first_reads.len());
    std::fs::remove_file(omit_size).unwrap();

    h.app
        .session_commands()
        .compact_session(&session)
        .await
        .unwrap();
    h.wait_session(&session, "busy").await;
    h.gate(&session, 1, "compact");
    h.wait_session(&session, "idle").await;
    write(&rows[2..]);
    assert!(matches!(
        service
            .page(session.clone(), TurnTimelineDirection::Backward, None, 20)
            .await,
        Err(pontia_agent_control::TurnTimelineServiceError::TimelineInvalid { .. })
    ));
    write(&[
        summary.clone(),
        command.clone(),
        rows[2].clone(),
        rows[3].clone(),
    ]);
    let page = service
        .page(session.clone(), TurnTimelineDirection::Backward, None, 20)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert!(
        page.items
            .iter()
            .all(|item| item.turn_id.as_deref() == Some(turns[1].0.as_str())
                && item.item.content_preview == "retained input")
    );
    assert!(
        service
            .page(
                session.clone(),
                TurnTimelineDirection::Backward,
                Some(turns[0].0.clone()),
                1
            )
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let cache = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(h.root.path().join("cache/session-timeline.db"))
            .read_only(true),
    )
    .await
    .unwrap();
    let generations: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT generation,count(*) FROM cached_turns WHERE session_id=? AND source_key='native' GROUP BY generation ORDER BY generation",
    ).bind(&session).fetch_all(&cache).await.unwrap();
    assert_eq!(generations.len(), 2);
    assert_eq!(generations[0], (0, 2));
    assert_eq!(generations[1].1, 2);
    let event: String = sqlx::query_scalar("SELECT event_id FROM events WHERE session_id=? AND event_type='session.compaction_completed'")
        .bind(&session).fetch_one(&h.app.db()).await.unwrap();
    let marker: String =
        sqlx::query_scalar("SELECT event_id FROM cached_compactions WHERE session_id=?")
            .bind(&session)
            .fetch_one(&cache)
            .await
            .unwrap();
    assert_eq!(marker, event);
    // Automatic compaction can remove the input in the middle of a Turn.
    write(&[summary.clone(), rows[3].clone()]);
    let page = service
        .page(session.clone(), TurnTimelineDirection::Backward, None, 20)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].turn_id.as_deref(), Some(turns[1].0.as_str()));
    assert_eq!(page.items[0].item.kind, "assistant");
    write(&[summary, command]);
    assert!(
        service
            .page(session.clone(), TurnTimelineDirection::Backward, None, 20)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    let receipt = h.resume_input(&session, "after compact resume").await;
    assert_eq!(receipt["inbox_message"]["state"], "dispatched");
    h.wait_inputs(&session, 3).await;
    h.gate(&session, 3, "start");
    h.gate(&session, 3, "finish");
    h.wait_states(&session, &["completed", "completed", "completed"])
        .await;
    let page = service
        .page(session.clone(), TurnTimelineDirection::Backward, None, 20)
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert!(
        page.items
            .iter()
            .all(|item| item.item.content_preview == "after compact resume")
    );
    assert_eq!(
        h.wire(&session)
            .into_iter()
            .filter(|f| f["method"] == "spawn")
            .map(|f| f["native"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!(binding.client_session_key),
            json!(binding.client_session_key)
        ]
    );
    h.stop().await;
}

#[tokio::test]
async fn manual_compaction_routes_to_host_and_reports_session_facts_without_a_turn() {
    let h = Harness::new().await;
    let session = h.create("before compaction").await;
    assert!(
        h.app
            .session_commands()
            .compact_session(&session)
            .await
            .is_err()
    );
    h.gate(&session, 1, "start");
    h.gate(&session, 1, "finish");
    h.wait_states(&session, &["completed"]).await;
    let receipt = h
        .app
        .session_commands()
        .compact_session(&session)
        .await
        .unwrap();
    assert!(receipt.accepted);
    h.wait_session(&session, "busy").await;
    assert!(
        h.app
            .session_commands()
            .compact_session(&session)
            .await
            .is_err()
    );
    h.gate(&session, 1, "compact");
    h.wait_session(&session, "idle").await;
    let facts: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT event_type, turn_id, payload FROM events WHERE session_id=? AND event_type LIKE 'session.compaction_%' ORDER BY rowid"
    ).bind(&session).fetch_all(&h.app.db()).await.unwrap();
    assert_eq!(
        facts.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(),
        ["session.compaction_started", "session.compaction_completed"]
    );
    for (_, turn, payload) in facts {
        assert!(turn.is_none());
        assert_eq!(
            serde_json::from_str::<Value>(&payload).unwrap()["operation_id"],
            receipt.operation_id
        );
    }
    assert_eq!(h.turns(&session).await.len(), 1);
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    assert!(
        h.app
            .session_commands()
            .compact_session(&session)
            .await
            .is_err()
    );
    let capabilities = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap()
        .capabilities;
    assert!(capabilities.compaction_events);
    assert!(!capabilities.manual_compaction);
    assert!(!capabilities.compaction_history);
    h.stop().await;
}

#[tokio::test]
async fn running_host_reattaches_its_turn_then_accepts_input_approvals_interrupt_and_exit() {
    let h = Harness::new().await;
    let session = h.create("held across daemon restart").await;
    h.gate(&session, 1, "start");
    let turn = h.wait_states(&session, &["running"]).await[0].0.clone();
    let fingerprint: String =
        sqlx::query_scalar("SELECT process_fingerprint FROM session_runtimes WHERE session_id=?")
            .bind(&session)
            .fetch_one(&h.app.db())
            .await
            .unwrap();
    let process: pontia_runtime::ProcessFingerprint = serde_json::from_str(&fingerprint).unwrap();
    let h = h.restart().await;
    assert_eq!(process.observe(), pontia_runtime::ProcessObservation::Alive);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !h.app.client_control().available(&session).await.unwrap()
            || h.wire(&session)
                .iter()
                .filter(|frame| frame["method"] == "attached")
                .count()
                != 2
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    h.gate(&session, 1, "finish");
    let completed = h.wait_states(&session, &["completed"]).await;
    assert_eq!(completed[0].0, turn);
    h.submit(&session, "approval").await;
    h.wait_inputs(&session, 2).await;
    h.gate(&session, 2, "start");
    let approvals = h.interactions(&session, &["pending"]).await;
    h.app
        .interactions()
        .respond(
            &h.app,
            &session,
            approvals[0]["interaction_id"].as_str().unwrap(),
            "allow",
        )
        .await
        .unwrap();
    h.interactions(&session, &["resolved"]).await;
    let turns = h.wait_states(&session, &["completed", "running"]).await;
    h.app
        .turn_commands()
        .interrupt_turn(&session, &turns[1].0)
        .await
        .unwrap();
    h.gate(&session, 2, "finish");
    h.wait_states(&session, &["completed", "interrupted"]).await;
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|frame| frame["method"] == "spawn")
            .count(),
        1
    );
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|frame| frame["method"] == "input")
            .count(),
        2
    );
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    assert_eq!(
        process.observe(),
        pontia_runtime::ProcessObservation::Exited
    );
    h.stop().await;
}

#[tokio::test]
async fn first_ready_reconnects_before_commit_and_after_a_lost_receipt() {
    for (input, starting) in [
        ("hold first ready", false),
        ("hold first ready", true),
        ("lose first ready receipt", false),
    ] {
        let h = Harness::new().await;
        let session = h.create(input).await;
        h.wait_inputs(&session, 1).await;
        if input == "hold first ready" {
            let capabilities = h
                .app
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap()
                .capabilities;
            assert!(!capabilities.reasoning_effort_control);
        }
        if starting {
            sqlx::query("UPDATE sessions SET state='starting' WHERE session_id=?")
                .bind(&session)
                .execute(&h.app.db())
                .await
                .unwrap();
        }
        if input == "lose first ready receipt" {
            h.wait_session(&session, "idle").await;
        }
        let h = h.restart().await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !h
                .wire(&session)
                .iter()
                .any(|f| f["method"] == "ready_replayed")
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        h.wait_session(&session, "idle").await;
        h.gate(&session, 1, "start");
        h.gate(&session, 1, "finish");
        h.wait_states(&session, &["completed"]).await;
        let ready: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM events WHERE session_id=? AND event_type='session.ready'",
        )
        .bind(&session)
        .fetch_one(&h.app.db())
        .await
        .unwrap();
        assert_eq!(ready, 1);
        h.stop().await;
    }
}

#[tokio::test]
async fn unconfirmed_first_ready_requires_the_original_unbound_launch() {
    for condition in [
        "idle",
        "exited",
        "runtime_running",
        "fingerprint",
        "foreign_binding",
    ] {
        let h = Harness::new().await;
        let session = h.create("hold first ready").await;
        h.wait_inputs(&session, 1).await;
        let pool = h.app.db();
        let original: String = sqlx::query_scalar(
            "SELECT process_fingerprint FROM session_runtimes WHERE session_id=?",
        )
        .bind(&session)
        .fetch_one(&pool)
        .await
        .unwrap();
        match condition {
            "runtime_running" => {
                sqlx::query("UPDATE session_runtimes SET state='running' WHERE session_id=?")
                    .bind(&session)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            "fingerprint" => {
                sqlx::query("UPDATE session_runtimes SET process_fingerprint=json_set(process_fingerprint,'$.start_time_seconds',0) WHERE session_id=?")
                    .bind(&session).execute(&pool).await.unwrap();
            }
            "foreign_binding" => {
                let native = h
                    .wire(&session)
                    .into_iter()
                    .find(|f| f["method"] == "spawn")
                    .unwrap()["native"]
                    .clone();
                let other = h.create("work").await;
                h.wait_session(&other, "idle").await;
                sqlx::query("UPDATE agent_bindings SET client_session_key=? WHERE session_id=?")
                    .bind(native.as_str().unwrap())
                    .bind(other)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            state => {
                sqlx::query("UPDATE sessions SET state=? WHERE session_id=?")
                    .bind(state)
                    .bind(&session)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
        }
        let h = h.restart().await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !h
                .wire(&session)
                .iter()
                .any(|f| f["method"] == "attach_error" && f["code"] == -32009)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("expected permanent rejection for {condition}"));
        sqlx::query("UPDATE session_runtimes SET process_fingerprint=? WHERE session_id=?")
            .bind(original)
            .bind(&session)
            .execute(&pool)
            .await
            .unwrap();
        h.stop().await;
    }
}
