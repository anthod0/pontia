use axum::{Json, Router, routing::post};
use serde_json::{Value, json};
use std::process::{Command, Output};

struct Fixture {
    root: tempfile::TempDir,
    requests: tokio::sync::mpsc::UnboundedReceiver<Value>,
    server: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new(data: Value) -> Self {
        let root = tempfile::tempdir().unwrap();
        let (sender, requests) = tokio::sync::mpsc::unbounded_channel();
        let app = Router::new().route(
            "/api/v1/bot/{endpoint}",
            post(
                move |headers: axum::http::HeaderMap, Json(request): Json<Value>| {
                    let sender = sender.clone();
                    let data = data.clone();
                    async move {
                        assert_eq!(headers["authorization"], "Bearer test-token");
                        sender.send(request).unwrap();
                        Json(json!({"data": data, "error": null, "meta": {}}))
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        std::fs::write(
            root.path().join("config.toml"),
            format!("bind_addr = \"{addr}\"\nexternal_api_token = \"test-token\"\n"),
        )
        .unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            root,
            requests,
            server,
        }
    }

    async fn cli(&self, args: &[&str]) -> Output {
        let root = self.root.path().to_path_buf();
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        tokio::task::spawn_blocking(move || {
            Command::new(env!("CARGO_BIN_EXE_pontia"))
                .arg("bot")
                .args(args)
                .env("PONTIA_HOME", root)
                .env("PONTIA_SESSION_ID", "caller-session")
                .output()
                .unwrap()
        })
        .await
        .unwrap()
    }

    async fn continue_page(&self, output: &Output) -> Output {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout.clone()).unwrap();
        let command = text
            .lines()
            .find(|line| line.starts_with("pontia bot "))
            .unwrap()
            .to_owned();
        let root = self.root.path().to_path_buf();
        tokio::task::spawn_blocking(move || {
            Command::new("bash")
                .args(["-c", &command])
                .current_dir(&root)
                .env(
                    "PATH",
                    format!(
                        "{}:/usr/bin:/bin",
                        std::path::Path::new(env!("CARGO_BIN_EXE_pontia"))
                            .parent()
                            .unwrap()
                            .display()
                    ),
                )
                .env("PONTIA_HOME", &root)
                .env("PONTIA_SESSION_ID", "caller-session")
                .output()
                .unwrap()
        })
        .await
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[tokio::test]
async fn session_pages_preserve_filters_source_and_opaque_anchors_in_text_and_json() {
    let next = json!({
        "session_id": "target-session", "turn_id": "turn ' $(touch injected)",
        "direction": "forward", "limit": 1,
        "categories": ["tool_call", "tool_result"], "history_source": "file"
    });
    // An empty filtered page can still require continuation.
    let data = json!({"session_id":"target-session", "items":[], "has_more":true, "next":next, "history_source":"file"});
    let mut f = Fixture::new(data.clone()).await;
    let args = [
        "session-messages",
        "--session-id",
        "target-session",
        "--direction",
        "forward",
        "--limit",
        "1",
        "--category",
        "tool_call",
        "--category",
        "tool_result",
    ];
    let first = f.cli(&args).await;
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let request = f.requests.try_recv().unwrap();
    assert_eq!(request["session_id"], "caller-session");
    assert_eq!(request["input"]["session_id"], "target-session");
    assert_eq!(
        request["input"]["categories"],
        json!(["tool_call", "tool_result"])
    );
    assert_eq!(request["input"]["direction"], "forward");
    assert_eq!(request["input"]["limit"], 1);
    let second = f.continue_page(&first).await;
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(f.requests.try_recv().unwrap()["input"], next);
    assert!(!f.root.path().join("injected").exists());
    let mut args = args.to_vec();
    args.push("--json");
    let structured = f.cli(&args).await;
    assert!(structured.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&structured.stdout).unwrap(),
        data
    );
}

#[tokio::test]
async fn session_text_retains_message_content_and_citation_ids() {
    let data = json!({"session_id":"target", "items":[{
        "turn_id":"turn-42", "item_id":"native:item:23", "role":"assistant",
        "kind":"message", "content_preview":"第一行\n第二行 'quoted'"
    }], "has_more":false, "next":null, "history_source":"native"});
    let mut f = Fixture::new(data).await;
    let output = f.cli(&["session-messages", "--session-id", "target"]).await;
    assert!(output.status.success());
    assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
    let text = String::from_utf8(output.stdout).unwrap();
    for value in [
        "target",
        "turn-42",
        "native:item:23",
        "第一行",
        "第二行 'quoted'",
    ] {
        assert!(text.contains(value));
    }
    let request = f.requests.try_recv().unwrap();
    assert_eq!(
        request["input"]["categories"],
        json!(["user", "assistant", "other"])
    );
    assert_eq!(request["input"]["direction"], "backward");
    assert_eq!(request["input"]["limit"], 10);
}

#[tokio::test]
async fn neighbor_continuation_preserves_origin_workspace_and_page_size() {
    let mut f =
        Fixture::new(json!({"memories":[], "next_cursor":"memory ' $(touch injected)"})).await;
    let first = f
        .cli(&[
            "memory",
            "neighbors",
            "--id",
            "origin",
            "--workspace-id",
            "project",
            "--limit",
            "2",
        ])
        .await;
    f.requests.try_recv().unwrap();
    let second = f.continue_page(&first).await;
    assert!(second.status.success());
    assert_eq!(
        f.requests.try_recv().unwrap()["command"],
        json!({
            "action":"neighbors", "input":{
                "id":"origin", "workspace_id":"project", "limit":2, "after":"memory ' $(touch injected)"
            }
        })
    );
    assert!(!f.root.path().join("injected").exists());
}

#[tokio::test]
async fn named_arguments_reject_incomplete_or_invalid_operations_before_sending_requests() {
    let mut f = Fixture::new(json!({})).await;
    for args in [
        vec!["update-user", "--old-text", ""],
        vec!["workspace", "describe", "--workspace-id", "project"],
        vec![
            "workspace",
            "describe",
            "--workspace-id",
            "project",
            "--description",
            "x",
            "--clear-description",
        ],
        vec![
            "memory",
            "create",
            "--kind",
            "invalid",
            "--content",
            "x",
            "--keyword",
            "x",
        ],
        vec!["memory", "search"],
        vec!["memory", "archive", "--id", "x"],
        vec!["memory", "archive", "--id", "x", "--archived", "maybe"],
        vec!["session-messages", "--session-id", "target", "--limit", "0"],
        vec![
            "session-messages",
            "--session-id",
            "target",
            "--category",
            "invalid",
        ],
    ] {
        let output = f.cli(&args).await;
        assert!(!output.status.success(), "accepted {args:?}");
        assert!(output.stdout.is_empty());
    }
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test]
async fn helper_commands_preserve_target_source_and_message_arguments() {
    let mut f = Fixture::new(json!({"delivery":{"status":"accepted"}})).await;
    for action in ["create", "retrieve", "send", "reply", "list", "close"] {
        let mut args = vec!["helper", action, "--json"];
        if matches!(action, "send" | "close") {
            args.extend(["--session-id", "helper-session"]);
        }
        if matches!(action, "create" | "retrieve" | "send" | "reply") {
            args.extend(["--message", "new\nrequirements"]);
        }
        if action == "retrieve" {
            args.extend(["--workspace-id", "project", "--profile-version", "2"]);
        }
        if action == "create" {
            args.extend(["--description", "Task", "--profile-id", "selected-profile"]);
        }
        let output = f.cli(&args).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let request = f.requests.recv().await.unwrap();
        assert_eq!(request["session_id"], "caller-session");
        assert_eq!(request["command"]["action"], action);
        if matches!(action, "send" | "close") {
            assert_eq!(request["command"]["input"]["session_id"], "helper-session");
        }
        if matches!(action, "create" | "retrieve" | "send" | "reply") {
            assert_eq!(request["command"]["input"]["message"], "new\nrequirements");
        }
        if action == "retrieve" {
            assert_eq!(request["command"]["input"]["workspace_id"], "project");
            assert_eq!(request["command"]["input"]["profile_version"], "2");
        }
    }
}

#[tokio::test]
async fn helper_check_continuation_preserves_the_native_history_query() {
    let next = json!({"session_id":"helper", "turn_id":"opaque ' anchor", "direction":"backward", "limit":1, "categories":["assistant"], "history_source":"file"});
    let mut f = Fixture::new(json!({"session":{"state":"busy"},"messages":{"next":next}})).await;
    let output = f.cli(&["helper", "check", "--session-id", "helper"]).await;
    let _ = f.requests.recv().await.unwrap();
    let continuation = f.continue_page(&output).await;
    assert!(continuation.status.success());
    let request = f.requests.recv().await.unwrap();
    assert_eq!(request["command"]["action"], "check");
    assert_eq!(request["command"]["input"], next);
}
