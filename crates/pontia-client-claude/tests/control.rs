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
    async fn stop(self) {
        self.app.shutdown().notify();
        tokio::time::timeout(Duration::from_secs(5), self.tasks.join())
            .await
            .unwrap()
            .unwrap();
    }
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
