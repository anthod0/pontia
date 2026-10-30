use pontia_application::{
    AppState, CreateSessionRequest, SubmitInboxMessageRequest,
    client_contract::ClientServicePhase,
    clients::{ClientRegistry, ClientServiceTasks},
};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::time::Duration;

struct Harness {
    app: AppState,
    root: tempfile::TempDir,
    tasks: ClientServiceTasks,
}
impl Harness {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = connect_sqlite("sqlite::memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();
        let mut clients = ClientRegistry::default();
        clients.register(pontia_client_claude::registration_with_host(Some(
            format!("{}/tests/fixtures/host.mjs", env!("CARGO_MANIFEST_DIR")).into(),
        )));
        let app = AppState::builder(pool, root.path().into())
            .clients(clients.clone())
            .build();
        pontia_application::agent_clients::AgentClientService::new(app.db(), clients.clone())
            .save("claude-agent", "default", Some(true), None)
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
                    branch_target_turn_id: None,
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
    async fn stop(self) {
        self.app.shutdown().notify();
        tokio::time::timeout(Duration::from_secs(5), self.tasks.join())
            .await
            .unwrap()
            .unwrap();
    }

    async fn history(
        &self,
        session: &str,
    ) -> Result<pontia_application::TurnTreeHistoryPage, pontia_application::TurnTimelineServiceError>
    {
        pontia_application::TurnTimelineService::new(self.app.event_ingest_service())
            .tree_history(session.into(), None, 50)
            .await
    }
    async fn resume_input(&self, session: &str, text: &str) -> Value {
        let message = uuid::Uuid::new_v4().to_string();
        let result = self
            .app
            .inbox_commands()
            .prepare_message_once(
                &self.app.session_commands(),
                &message,
                session,
                text.into(),
                json!({}),
            )
            .await
            .unwrap();
        if result.data["inbox_message"]["state"] == "resuming" {
            let runtime: String =
                sqlx::query_scalar("SELECT runtime_id FROM session_runtimes WHERE session_id=?")
                    .bind(session)
                    .fetch_one(&self.app.db())
                    .await
                    .unwrap();
            self.app
                .inbox_commands()
                .release_prepared_message(session, &message, &runtime)
                .await
                .unwrap();
        }
        result.data
    }
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
        assert_eq!(result["inbox_message"]["state"], "resuming", "{result}");
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
    let result = pontia_application::TurnTimelineService::new(h.app.event_ingest_service())
        .page(
            session.clone(),
            pontia_application::TurnTimelineDirection::Backward,
            None,
            50,
        )
        .await;
    assert!(matches!(
        result,
        Err(pontia_application::TurnTimelineServiceError::CapabilityUnavailable)
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
    let binding = pontia_application::AgentBindingService::new(h.app.db())
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
    assert_eq!(result["inbox_message"]["state"], "resuming", "{result}");
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
    let app = AppState::builder(h.app.db(), h.root.path().into())
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
    assert_eq!(result["inbox_message"]["state"], "resuming", "{result}");
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
    let binding = pontia_application::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    // Remove only this test's native-source fixture under its owned TempDir.
    std::fs::remove_file(
        h.root
            .path()
            .join(format!("history-{}.jsonl", binding.client_session_key)),
    )
    .unwrap();
    assert!(matches!(
        h.history(&session).await,
        Err(pontia_application::TurnTimelineServiceError::SourceUnavailable)
    ));
    let outcome = h.resume_input(&session, "new real input").await;
    assert_eq!(outcome["inbox_message"]["state"], "failed");
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|v| v["method"] == "spawn")
            .count(),
        1
    );
    assert_eq!(h.turns(&session).await.len(), 1);
    assert_eq!(
        pontia_application::AgentBindingService::new(h.app.db())
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
async fn host_loss_keeps_history_readable_and_does_not_authorize_parallel_resume() {
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
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let view = serde_json::to_value(
                h.app
                    .queries()
                    .get_session(&session)
                    .await
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
            if view["claude-agent"]["unavailable_reason"] == "host_unavailable" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        h.app
            .session_commands()
            .resume_session(&session, h.root.path())
            .await
            .is_err()
    );
    assert_eq!(
        h.history(&session).await.unwrap().groups[0].items[0]
            .item
            .content_preview,
        "unfinished"
    );
    h.app.inbox_commands().notify_available(&session);
    h.app.inbox_commands().drain_inbox(&session).await.unwrap();
    assert_eq!(h.turns(&session).await[0].1, "running");
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|v| v["method"] == "spawn")
            .count(),
        1
    );
    assert_eq!(
        h.wire(&session)
            .iter()
            .filter(|v| v["method"] == "input")
            .count(),
        1
    );
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
    service.respond(&h.app, &session, b, "deny").await.unwrap();
    let resolved = h.interactions(&session, &["resolved", "resolved"]).await;
    h.wait_session(&session, "busy").await;
    service.respond(&h.app, &session, a, "allow").await.unwrap();
    let choices: Vec<_> = h
        .wire(&session)
        .into_iter()
        .filter(|v| v["method"] == "permission")
        .collect();
    assert_eq!(choices.len(), 2);
    assert_ne!(choices[0]["permission_id"], choices[1]["permission_id"]);
    assert_eq!(choices[0]["option"], resolved[0]["response"]["option_key"]);
    assert_eq!(choices[1]["option"], "deny");
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
        assert_eq!(h.turns(&session).await[0].1, "running");
        assert_eq!(
            h.wire(&session)
                .iter()
                .filter(|v| v["method"] == "permission")
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
    assert!(!h.wire(&b).iter().any(|v| v["method"] == "permission"));
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
    let binding = pontia_application::AgentBindingService::new(h.app.db())
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
        pontia_application::AgentBindingService::new(h.app.db())
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
        h.wait_inputs(&session, 2).await;
        h.stop().await;
    }
}

#[tokio::test]
async fn explicit_sdk_close_abandons_active_turn_without_fabricating_result() {
    let h = Harness::new().await;
    let session = h.create("held").await;
    h.gate(&session, 1, "start");
    h.wait_states(&session, &["running"]).await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    h.wait_states(&session, &["abandoned"]).await;
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
            matches!(&snapshot.items[..], [pontia_application::LiveOutputItem::AssistantText { text: value, .. }] if value == text)
        );
    }
    let bindings = pontia_application::AgentBindingService::new(h.app.db());
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
async fn failed_tool_does_not_fail_turn_and_explicit_failure_does() {
    for (text, expected) in [("tool", "completed"), ("fail", "failed")] {
        let h = Harness::new().await;
        let session = h.create(text).await;
        h.gate(&session, 1, "start");
        h.wait_states(&session, &["running"]).await;
        h.gate(&session, 1, "finish");
        h.wait_states(&session, &[expected]).await;
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
    h.wait_states(&session, &["abandoned"]).await;
    h.stop().await;
}
