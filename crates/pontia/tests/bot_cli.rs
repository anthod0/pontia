use axum::{Json, Router, routing::post};
use serde_json::{Value, json};
use std::process::{Command, Output};

struct Fixture {
    root: tempfile::TempDir,
    requests: tokio::sync::mpsc::UnboundedReceiver<Value>,
    server: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new(endpoint: &str, data: Value) -> Self {
        let root = tempfile::tempdir().unwrap();
        let (sender, requests) = tokio::sync::mpsc::unbounded_channel();
        let app = Router::new().route(
            &format!("/api/v1/bot/{endpoint}"),
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
    let mut f = Fixture::new("session-messages", data.clone()).await;
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
    let mut f = Fixture::new("session-messages", data).await;
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
    let mut f = Fixture::new(
        "memory",
        json!({"memories":[], "next_cursor":"memory ' $(touch injected)"}),
    )
    .await;
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
    let mut f = Fixture::new("{endpoint}", json!({})).await;
    for args in [
        vec![
            "helper",
            "create",
            "--title",
            "Task",
            "--description",
            "task",
            "--message",
            "start",
        ],
        vec![
            "helper",
            "create",
            "--title",
            "Task",
            "--description",
            "task",
            "--message",
            "start",
            "--workspace-id",
            "project",
            "--temporary",
            "task",
        ],
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
        vec!["helper", "retrieve", "--message", "question"],
        vec!["helper", "compact"],
        vec!["memory", "retrieve"],
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
    let mut f = Fixture::new("helper", json!({"delivery":{"status":"accepted"}})).await;
    for action in ["create", "send", "reply", "list", "compact", "close"] {
        let mut args = vec!["helper", action, "--json"];
        if matches!(action, "send" | "compact" | "close") {
            args.extend(["--session-id", "helper-session"]);
        }
        if matches!(action, "create" | "send" | "reply") {
            args.extend(["--message", "new\nrequirements"]);
        }
        if action == "create" {
            args.extend(["--workspace-id", "project"]);
            args.extend(["--description", "Task", "--profile-id", "selected-profile"]);
            args.extend(["--title", "核对文件下载"]);
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
        if matches!(action, "send" | "compact" | "close") {
            assert_eq!(request["command"]["input"]["session_id"], "helper-session");
        }
        if matches!(action, "create" | "send" | "reply") {
            assert_eq!(request["command"]["input"]["message"], "new\nrequirements");
        }
        if action == "create" {
            assert_eq!(request["command"]["input"]["workspace_id"], "project");
            assert_eq!(
                request["command"]["input"]["profile_id"],
                "selected-profile"
            );
            assert_eq!(request["command"]["input"]["title"], "核对文件下载");
        }
    }
}

#[tokio::test]
async fn memory_retrieve_starts_a_helper_with_the_requested_scope() {
    let launch = json!({"session_id":"retrieval-session", "delivery":{"status":"accepted"}});
    let mut f = Fixture::new("helper", launch.clone()).await;
    let output = f
        .cli(&[
            "memory",
            "retrieve",
            "--message",
            "Explain the release decision",
            "--workspace-id",
            "project",
            "--json",
        ])
        .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        launch
    );
    assert_eq!(
        f.requests.try_recv().unwrap(),
        json!({
            "session_id":"caller-session",
            "command":{"action":"retrieve", "input":{
                "message":"Explain the release decision", "workspace_id":"project"
            }}
        })
    );
}

#[tokio::test]
async fn temporary_helper_sends_the_task_directory_name_as_the_location() {
    let mut f = Fixture::new("helper", json!({"delivery":{"status":"accepted"}})).await;
    let name = "2026-11-20_汇率查询";
    let output = f
        .cli(&[
            "helper",
            "create",
            "--title",
            "Checking exchange rates",
            "--temporary",
            name,
            "--description",
            name,
            "--message",
            "Find the exchange rate",
        ])
        .await;
    assert!(output.status.success());
    assert_eq!(
        f.requests.try_recv().unwrap()["command"]["input"],
        json!({
            "temporary":name, "description":name, "message":"Find the exchange rate",
            "profile_id":null, "title":"Checking exchange rates"
        })
    );
}

#[tokio::test]
async fn helper_check_continuation_preserves_the_native_history_query() {
    let next = json!({"session_id":"helper", "turn_id":"opaque ' anchor", "direction":"backward", "limit":1, "categories":["assistant"], "history_source":"file"});
    let mut f = Fixture::new(
        "helper",
        json!({"session":{"state":"busy"},"messages":{"next":next}}),
    )
    .await;
    let output = f.cli(&["helper", "check", "--session-id", "helper"]).await;
    let _ = f.requests.recv().await.unwrap();
    let continuation = f.continue_page(&output).await;
    assert!(continuation.status.success());
    let request = f.requests.recv().await.unwrap();
    assert_eq!(request["command"]["action"], "check");
    assert_eq!(request["command"]["input"], next);
}

#[tokio::test]
async fn helper_create_rejects_removed_profile_version_argument() {
    let mut f = Fixture::new("helper", json!({})).await;
    let output = f
        .cli(&[
            "helper",
            "create",
            "--workspace-id",
            "project",
            "--profile-id",
            "reviewer",
            "--title",
            "Review",
            "--profile-version",
            "1",
            "--description",
            "Review",
            "--message",
            "Review changes",
        ])
        .await;
    assert!(!output.status.success());
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test]
async fn helper_requires_a_title_before_contacting_the_service() {
    let mut f = Fixture::new("helper", json!({})).await;
    let output = f
        .cli(&[
            "helper",
            "create",
            "--temporary",
            "task",
            "--description",
            "Task",
            "--message",
            "Start",
        ])
        .await;
    assert!(!output.status.success());
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test]
async fn send_to_user_rejects_the_removed_working_on_argument() {
    let mut f = Fixture::new("send-to-user", json!({})).await;
    let output = f
        .cli(&["send-to-user", "--message", "Hello", "--working-on", "Task"])
        .await;
    assert!(!output.status.success());
    assert!(f.requests.try_recv().is_err());
}

#[tokio::test]
async fn send_to_user_forwards_todo_and_repeated_link_and_resolution_targets() {
    let mut f = Fixture::new("send-to-user", json!({"sent": true, "message_id": "new"})).await;
    let output = f
        .cli(&[
            "send-to-user",
            "--message",
            "Hello",
            "--related-to",
            "a",
            "--related-to",
            "b",
            "--todo",
            "--resolve-todo",
            "c",
            "--resolve-todo",
            "d",
        ])
        .await;
    assert!(output.status.success());
    assert_eq!(
        f.requests.recv().await.unwrap()["input"],
        json!({"message": "Hello", "related_to": ["a", "b"], "todo": true, "resolve_todos": ["c", "d"]})
    );
}

#[tokio::test]
async fn link_message_requires_targets_and_forwards_all_of_them() {
    let mut f = Fixture::new(
        "link-message",
        json!({"message_id": "m","related_to":["a","b","c"]}),
    )
    .await;
    let missing = f.cli(&["link-message", "--message-id", "m"]).await;
    assert!(!missing.status.success());
    assert!(f.requests.try_recv().is_err());
    let output = f
        .cli(&[
            "link-message",
            "--message-id",
            "m",
            "--related-to",
            "a",
            "b",
            "--related-to",
            "c",
        ])
        .await;
    assert!(output.status.success());
    assert_eq!(
        f.requests.recv().await.unwrap(),
        json!({
            "session_id":"caller-session", "input":{"message_id":"m","related_to":["a","b","c"]}
        })
    );
}

#[tokio::test]
async fn set_description_requires_and_forwards_the_description() {
    let mut fixture =
        Fixture::new("description", json!({"description":"- Release planning"})).await;
    let missing = fixture.cli(&["set-description"]).await;
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--description"));
    let result = fixture
        .cli(&["set-description", "--description", "- Release planning"])
        .await;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fixture.requests.recv().await.unwrap(),
        json!({
            "session_id":"caller-session", "input":{"description":"- Release planning"}
        })
    );
}

#[tokio::test]
async fn profile_commands_send_grouped_actions_and_reject_incomplete_updates() {
    let mut f = Fixture::new("profile", json!({})).await;
    let target = json!({"profile_id":"review"});
    let client = json!({"client_type":"acp","client_key":"vendor/grok"});
    for (args, expected) in [
        (vec!["list"], json!({"action":"list"})),
        (vec!["clients"], json!({"action":"clients"})),
        (
            vec!["show", "--profile-id", "review"],
            json!({"action":"show","input":target}),
        ),
        (
            vec!["delete", "--profile-id", "review"],
            json!({"action":"delete","input":target}),
        ),
        (
            vec!["models", "--client", "acp/vendor/grok"],
            json!({"action":"models","input":{"client":client}}),
        ),
        (
            vec![
                "create",
                "--profile-id",
                "review",
                "--name",
                "Review",
                "--description",
                "Check code",
                "--client",
                "acp/vendor/grok",
                "--model",
                "m",
                "--reasoning-effort",
                "high",
                "--system-prompt",
                "- Review changes",
                "--use-yolo",
            ],
            json!({"action":"create","input":{"profile_id":"review","name":"Review","description":"Check code","client":client,"model":"m","reasoning_effort":"high","system_prompt":"- Review changes","use_yolo":true}}),
        ),
        (
            vec![
                "update",
                "--profile-id",
                "review",
                "--clear-description",
                "--clear-system-prompt",
                "--clear-reasoning-effort",
                "--use-yolo",
                "false",
            ],
            json!({"action":"update","input":{"profile_id":"review","name":null,"description":null,"system_prompt":null,"client":null,"model":null,"clear_description":true,"clear_system_prompt":true,"reasoning_effort":null,"clear_reasoning_effort":true,"use_yolo":false}}),
        ),
    ] {
        let args = [&["profile"][..], &args[..]].concat();
        let output = f.cli(&args).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            f.requests.recv().await.unwrap(),
            json!({"session_id":"caller-session","command":expected})
        );
    }
    for flags in [
        vec![],
        vec!["--client", "acp/vendor/grok"],
        vec!["--description", "x", "--clear-description"],
        vec!["--reasoning-effort", "low", "--clear-reasoning-effort"],
        vec!["--system-prompt", "x", "--clear-system-prompt"],
        vec!["--client", "acp", "--model", "m"],
        vec!["--client", "acp/", "--model", "m"],
    ] {
        let args = [
            &["profile", "update", "--profile-id", "review"][..],
            &flags[..],
        ]
        .concat();
        assert!(!f.cli(&args).await.status.success(), "{args:?}");
    }
    assert!(f.requests.try_recv().is_err());
}

#[test]
fn profile_templates_work_offline_without_a_session() {
    let root = tempfile::tempdir().unwrap();
    let cli = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_pontia"))
            .arg("bot")
            .args(args)
            .env("PONTIA_HOME", root.path())
            .env_remove("PONTIA_SESSION_ID")
            .env_remove("PONTIA_EXTERNAL_API_TOKEN")
            .env("PONTIA_AUTH_ORIGIN", "https://127.0.0.1:1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        String::from_utf8(output.stdout).unwrap()
    };
    assert!(cli(&["profile", "templates"]).starts_with("# Agent Profile Templates\n"));
    for args in [
        ["profile", "templates", "--json"],
        ["--json", "profile", "templates"],
    ] {
        assert_eq!(
            cli(&args),
            include_str!("../../../assets/agent-profile-templates.json")
        );
    }
    for args in [vec!["profile", "-h"], vec!["profile", "create", "-h"]] {
        assert!(cli(&args).contains("pontia bot profile templates"));
    }
}
