use std::{
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Output, Stdio},
    sync::Arc,
};

use pontia_application::AppState;
use pontia_bot::memory::{Error, MemoryAccess, MemoryIdentity, MemoryRole, Result};
use pontia_http::{HttpState, router};
use serde_json::{Value, json};

struct BoundSessions;
impl MemoryIdentity for BoundSessions {
    fn resolve<'a>(
        &'a self,
        session: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<MemoryAccess>> + Send + 'a>> {
        Box::pin(async move {
            let (role, workspace_id) = match session {
                "bot-session" => (MemoryRole::Bot, None),
                "dreaming-session" => (MemoryRole::Dreaming, None),
                "reader-session" => (MemoryRole::Retrieval, Some("project".into())),
                _ => return Err(Error::Forbidden),
            };
            Ok(MemoryAccess { role, workspace_id })
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
            state = state.with_memory_identity(Arc::new(BoundSessions));
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
    async fn cli(&self, session: Option<&str>, action: &str, input: Value, file: bool) -> Output {
        let root = self.root.path().to_path_buf();
        let session = session.map(str::to_owned);
        let action = action.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut command = Command::new(env!("CARGO_BIN_EXE_pontia"));
            command
                .args(["bot", "memory", &action])
                .env("PONTIA_HOME", &root)
                .env_remove("PONTIA_SESSION_ID");
            if let Some(session) = session {
                command.env("PONTIA_SESSION_ID", session);
            }
            if file {
                let path = root.join("input.json");
                std::fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
                command.arg("--input").arg(path).output().unwrap()
            } else {
                let mut child = command
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                let _ = child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(&serde_json::to_vec(&input).unwrap());
                child.wait_with_output().unwrap()
            }
        })
        .await
        .unwrap()
    }
    async fn call(&self, session: &str, action: &str, input: Value) -> Value {
        let output = self.cli(Some(session), action, input, false).await;
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
    let output = f.cli(Some("bot-session"),"create",json!({"kind":"fact","content":"项目使用 Rust","keywords":["项目","Rust"],"workspace_id":"project"}),true).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let created: Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = created["memory"]["id"].clone();
    let search = f
        .call("reader-session", "search", json!({"keywords":["RUST"]}))
        .await;
    assert_eq!(search["results"][0]["memory"]["id"], id);
    assert_eq!(search["results"][0]["memory"]["content"], "项目使用 Rust");
    assert_eq!(search["has_more"], false);
    let hidden = f
        .call(
            "bot-session",
            "create",
            json!({"kind":"fact","content":"global","keywords":["Rust"]}),
        )
        .await;
    let read = f
        .call(
            "reader-session",
            "read",
            json!({"ids":[id,hidden["memory"]["id"]]}),
        )
        .await;
    assert_eq!(read["memories"].as_array().unwrap().len(), 1);
    assert!(read["memories"][0]["last_read_at"].is_null());
    f.call("bot-session", "use", json!({"ids":[id]})).await;
    let used = f.call("reader-session", "read", json!({"ids":[id]})).await;
    assert!(used["memories"][0]["last_read_at"].is_string());
    assert_eq!(
        used["memories"][0]["updated_at"],
        created["memory"]["updated_at"]
    );
    f.call(
        "dreaming-session",
        "update",
        json!({"id":id,"kind":"decision","content":"now Go","keywords":["Go"],"importance":"high"}),
    )
    .await;
    assert_eq!(
        f.call("reader-session", "search", json!({"keywords":["Rust"]}))
            .await["results"],
        json!([])
    );
    assert_eq!(
        f.call("reader-session", "search", json!({"keywords":["Go"]}))
            .await["results"][0]["memory"]["id"],
        id
    );
    f.call(
        "dreaming-session",
        "link",
        json!({"memory_a_id":id,"memory_b_id":hidden["memory"]["id"]}),
    )
    .await;
    assert_eq!(
        f.call("bot-session", "neighbors", json!({"id":id})).await["memories"][0]["id"],
        hidden["memory"]["id"]
    );
    assert_eq!(
        f.call("reader-session", "neighbors", json!({"id":id}))
            .await["memories"],
        json!([])
    );
    f.call(
        "dreaming-session",
        "archive",
        json!({"id":id,"archived":true}),
    )
    .await;
    assert_eq!(
        f.call("reader-session", "read", json!({"ids":[id]})).await["memories"],
        json!([])
    );
    f.call(
        "dreaming-session",
        "archive",
        json!({"id":id,"archived":false}),
    )
    .await;
    assert_eq!(
        f.call("reader-session", "read", json!({"ids":[id]})).await["memories"][0]["content"],
        "now Go"
    );
    for (session, action, input) in [
        (
            "reader-session",
            "create",
            json!({"kind":"fact","content":"bad","keywords":["bad"]}),
        ),
        ("reader-session", "use", json!({"ids":[id]})),
        ("bot-session", "archive", json!({"id":id,"archived":true})),
        ("unknown-session", "search", json!({"keywords":["Go"]})),
    ] {
        let output = f.cli(Some(session), action, input, false).await;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("forbidden"));
    }
    let invalid = f
        .cli(
            Some("bot-session"),
            "search",
            json!({"keywords":["Go"],"limit":31}),
            false,
        )
        .await;
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("invalid_request"));
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
    let absent = f
        .cli(None, "search", json!({"keywords":["secret"]}), false)
        .await;
    assert!(!absent.status.success());
    assert!(String::from_utf8_lossy(&absent.stderr).contains("PONTIA_SESSION_ID"));
    let unbound = f
        .cli(
            Some("bot-session"),
            "search",
            json!({"keywords":["secret"]}),
            false,
        )
        .await;
    assert!(!unbound.status.success());
    assert!(String::from_utf8_lossy(&unbound.stderr).contains("forbidden"));
}
