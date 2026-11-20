use pontia_bot::identity::{BotAccess, BotIdentity, BotRole, IdentityError};
use std::{
    future::Future,
    pin::Pin,
    process::{Command, Output},
    sync::Arc,
};

use pontia_application::AppState;

use pontia_http::{HttpState, router};
use serde_json::{Value, json};

struct BoundSessions;
impl BotIdentity for BoundSessions {
    fn resolve<'a>(
        &'a self,
        session: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<BotAccess, IdentityError>> + Send + 'a>>
    {
        Box::pin(async move {
            let (role, workspace_id) = match session {
                "bot-session" => (BotRole::Bot, None),
                "dreaming-session" => (BotRole::Dreaming, None),
                "helper-session" => (BotRole::Helper, Some("project".into())),
                "reader-session" => (BotRole::Retrieval, Some("project".into())),
                _ => return Err(IdentityError),
            };
            Ok(BotAccess { role, workspace_id })
        })
    }
}

struct Fixture {
    root: tempfile::TempDir,
    addr: std::net::SocketAddr,
    server: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn new(bound: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let db = pontia_storage_sqlite::connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("test.db").display()
        ))
        .await
        .unwrap();
        pontia_storage_sqlite::run_migrations(&db).await.unwrap();
        let control =
            pontia_agent_control::AgentControl::builder(db, root.path().to_path_buf()).build();
        let app = AppState::builder(control)
            .external_api_token(Some("test-token".into()))
            .build();
        let mut state = HttpState::from(app);
        if bound {
            state = state.with_bot_identity(Arc::new(BoundSessions));
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        std::fs::write(
            root.path().join("config.toml"),
            format!("bind_addr = \"{addr}\"\nexternal_api_token = \"test-token\"\n"),
        )
        .unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        });
        Self { root, addr, server }
    }
    async fn cli(&self, session: Option<&str>, args: &[&str]) -> Output {
        let root = self.root.path().to_path_buf();
        let session = session.map(str::to_owned);
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        tokio::task::spawn_blocking(move || {
            let mut command = Command::new(env!("CARGO_BIN_EXE_pontia"));
            command
                .args(["bot", "memory"])
                .args(&args)
                .env("PONTIA_HOME", &root)
                .env_remove("PONTIA_SESSION_ID");
            if let Some(session) = session {
                if session == "dreaming-session" && !args.iter().any(|arg| arg == "--reason") {
                    command.args(["--reason", "Test evidence"]);
                }
                command.env("PONTIA_SESSION_ID", session);
            }
            command.output().unwrap()
        })
        .await
        .unwrap()
    }
    async fn call(&self, session: &str, args: &[&str]) -> Value {
        let mut args = args.to_vec();
        args.push("--json");
        let output = self.cli(Some(session), &args).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[tokio::test]
async fn cli_runs_memory_lifecycle_through_authenticated_http_and_sqlite() {
    let f = Fixture::new(true).await;
    let content = "项目使用 Rust\n保留 '引号'、$变量 和 \"双引号\"";
    let created = f
        .call(
            "bot-session",
            &[
                "create",
                "--kind",
                "fact",
                "--content",
                content,
                "--keyword",
                "项目",
                "--keyword",
                "Rust",
                "--workspace-id",
                "project",
                "--session-id",
                "source-session",
                "--sources",
                "用户的原始说明",
                "--occurred-at",
                "2026-11-01T08:00:00+08:00",
            ],
        )
        .await;
    let id = created["memory"]["id"].as_str().unwrap();
    assert_eq!(created["memory"]["content"], content);
    assert_eq!(created["memory"]["session_id"], "source-session");
    assert_eq!(created["memory"]["sources"], "用户的原始说明");
    assert_eq!(created["memory"]["occurred_at"], "2026-11-01T00:00:00Z");
    let search = f
        .call("reader-session", &["search", "--keyword", "RUST"])
        .await;
    assert_eq!(search["results"][0]["memory"]["id"], id);
    assert_eq!(search["results"][0]["memory"]["content"], content);
    assert_eq!(search["has_more"], false);
    let text = f
        .cli(Some("reader-session"), &["search", "--keyword", "Rust"])
        .await;
    assert!(text.status.success());
    assert!(serde_json::from_slice::<Value>(&text.stdout).is_err());
    let text = String::from_utf8(text.stdout).unwrap();
    for value in [id, "项目使用 Rust", "用户的原始说明", "source-session"] {
        assert!(text.contains(value));
    }
    let hidden = f
        .call(
            "bot-session",
            &[
                "create",
                "--kind",
                "fact",
                "--content",
                "global",
                "--keyword",
                "Rust",
            ],
        )
        .await;
    let hidden_id = hidden["memory"]["id"].as_str().unwrap();
    let read = f
        .call("reader-session", &["read", "--id", id, "--id", hidden_id])
        .await;
    assert_eq!(read["memories"].as_array().unwrap().len(), 1);
    assert!(read["memories"][0]["last_read_at"].is_null());
    f.call("bot-session", &["use", "--id", id]).await;
    let used = f.call("reader-session", &["read", "--id", id]).await;
    assert!(used["memories"][0]["last_read_at"].is_string());
    assert_eq!(
        used["memories"][0]["updated_at"],
        created["memory"]["updated_at"]
    );
    f.call(
        "dreaming-session",
        &[
            "update",
            "--id",
            id,
            "--kind",
            "decision",
            "--content",
            "now Go",
            "--keyword",
            "Go",
            "--importance",
            "high",
        ],
    )
    .await;
    assert_eq!(
        f.call("reader-session", &["search", "--keyword", "Rust"])
            .await["results"],
        json!([])
    );
    assert_eq!(
        f.call("reader-session", &["search", "--keyword", "Go"])
            .await["results"][0]["memory"]["id"],
        id
    );
    let updated = f.call("reader-session", &["read", "--id", id]).await;
    assert!(updated["memories"][0]["sources"].is_null());
    assert!(updated["memories"][0]["occurred_at"].is_null());
    f.call(
        "dreaming-session",
        &["link", "--memory-a-id", id, "--memory-b-id", hidden_id],
    )
    .await;
    assert_eq!(
        f.call("bot-session", &["neighbors", "--id", id]).await["memories"][0]["id"],
        hidden_id
    );
    assert_eq!(
        f.call("reader-session", &["neighbors", "--id", id]).await["memories"],
        json!([])
    );
    f.call(
        "dreaming-session",
        &["unlink", "--memory-a-id", id, "--memory-b-id", hidden_id],
    )
    .await;
    assert_eq!(
        f.call("bot-session", &["neighbors", "--id", id]).await["memories"],
        json!([])
    );
    f.call(
        "dreaming-session",
        &["archive", "--id", id, "--archived", "true"],
    )
    .await;
    assert_eq!(
        f.call("reader-session", &["read", "--id", id]).await["memories"],
        json!([])
    );
    f.call(
        "dreaming-session",
        &["archive", "--id", id, "--archived", "false"],
    )
    .await;
    assert_eq!(
        f.call("reader-session", &["read", "--id", id]).await["memories"][0]["content"],
        "now Go"
    );
    for (session, args) in [
        (
            "reader-session",
            vec![
                "create",
                "--kind",
                "fact",
                "--content",
                "bad",
                "--keyword",
                "bad",
            ],
        ),
        ("reader-session", vec!["use", "--id", id]),
        (
            "bot-session",
            vec!["archive", "--id", id, "--archived", "true"],
        ),
        ("helper-session", vec!["search", "--keyword", "Go"]),
        ("helper-session", vec!["retrieve", "--message", "question"]),
        ("reader-session", vec!["retrieve", "--message", "question"]),
        ("unknown-session", vec!["search", "--keyword", "Go"]),
    ] {
        let output = f.cli(Some(session), &args).await;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("forbidden"));
    }
    let invalid = f
        .cli(
            Some("bot-session"),
            &["search", "--keyword", "Go", "--limit", "31"],
        )
        .await;
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
}

#[tokio::test]
async fn production_entrypoint_denies_unbound_and_caller_supplied_roles() {
    let f = Fixture::new(false).await;
    let request = json!({"session_id":"bot-session","command":{"action":"search","input":{"keywords":["secret"]}}});
    let client = reqwest::Client::new();
    let url = format!("http://{}/api/v1/bot/memory", f.addr);
    assert_eq!(
        client
            .post(&url)
            .json(&request)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let denied = client
        .post(&url)
        .bearer_auth("test-token")
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    assert_eq!(
        denied.json::<Value>().await.unwrap()["error"]["code"],
        "forbidden"
    );
    let mut forged = request;
    forged["role"] = json!("dreaming");
    assert_eq!(
        client
            .post(&url)
            .bearer_auth("test-token")
            .json(&forged)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let absent = f.cli(None, &["search", "--keyword", "secret"]).await;
    assert!(!absent.status.success());
    assert!(String::from_utf8_lossy(&absent.stderr).contains("PONTIA_SESSION_ID"));
    let unbound = f
        .cli(Some("bot-session"), &["search", "--keyword", "secret"])
        .await;
    assert!(!unbound.status.success());
    assert!(String::from_utf8_lossy(&unbound.stderr).contains("forbidden"));
}

#[tokio::test]
async fn dreaming_cli_pages_pending_entries_and_marks_completion() {
    let f = Fixture::new(true).await;
    let a = f
        .call(
            "bot-session",
            &[
                "create",
                "--kind",
                "fact",
                "--content",
                "a",
                "--keyword",
                "a",
            ],
        )
        .await;
    let b = f
        .call(
            "bot-session",
            &[
                "create",
                "--kind",
                "fact",
                "--content",
                "b",
                "--keyword",
                "b",
            ],
        )
        .await;
    let a = a["memory"]["id"].as_str().unwrap();
    let b = b["memory"]["id"].as_str().unwrap();
    let first = f
        .call("dreaming-session", &["pending", "--limit", "1"])
        .await;
    assert_eq!(first["memories"][0]["id"], a);
    let second = f
        .call(
            "dreaming-session",
            &["pending", "--after", first["next_cursor"].as_str().unwrap()],
        )
        .await;
    assert_eq!(second["memories"][0]["id"], b);
    f.call(
        "dreaming-session",
        &[
            "organize",
            "--id",
            a,
            "--organized",
            "true",
            "--reason",
            "Checked source and observations",
        ],
    )
    .await;
    let pending = f.call("dreaming-session", &["pending"]).await;
    assert_eq!(pending["memories"].as_array().unwrap().len(), 1);
    assert_eq!(pending["memories"][0]["id"], b);
    assert!(
        !f.cli(
            Some("bot-session"),
            &[
                "organize",
                "--id",
                b,
                "--organized",
                "true",
                "--reason",
                "not authorized"
            ]
        )
        .await
        .status
        .success()
    );
}
