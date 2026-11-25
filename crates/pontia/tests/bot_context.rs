use pontia_agent_control::ControlQueryService;
use pontia_application::AppState;
use pontia_bot::identity::{BotAccess, BotIdentity, BotRole, IdentityError};
use pontia_config::{WorkspaceBrowserConfig, WorkspaceRootConfig};
use pontia_http::{HttpState, router};
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    future::Future,
    pin::Pin,
    process::{Command, Output},
    sync::Arc,
};

struct Sessions;
impl BotIdentity for Sessions {
    fn resolve<'a>(
        &'a self,
        session: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<BotAccess, IdentityError>> + Send + 'a>> {
        Box::pin(async move {
            let role = match session {
                "bot" => BotRole::Bot,
                "helper" => BotRole::Helper,
                "dreaming" => BotRole::Dreaming,
                _ => return Err(IdentityError),
            };
            Ok(BotAccess {
                role,
                workspace_id: None,
            })
        })
    }
}

struct Fixture {
    root: tempfile::TempDir,
    app: AppState,
    addr: std::net::SocketAddr,
    server: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn new(bound: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let projects = root.path().join("projects");
        std::fs::create_dir_all(projects.join("app")).unwrap();
        std::fs::write(projects.join("file"), "not a directory").unwrap();
        std::os::unix::fs::symlink(projects.join("app"), projects.join("alias")).unwrap();
        std::os::unix::fs::symlink(root.path(), projects.join("escape")).unwrap();
        let db = pontia_storage_sqlite::connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("test.db").display()
        ))
        .await
        .unwrap();
        pontia_storage_sqlite::run_migrations(&db).await.unwrap();
        if bound {
            for session in ["bot", "helper", "dreaming"] {
                sqlx::query("INSERT INTO sessions(session_id,client_type,state,credential_env,credential_sha256) VALUES (?,'generic','idle','PONTIA_BOT_TOOL_TOKEN',?)")
                    .bind(session).bind(sha2::Sha256::digest(session).to_vec()).execute(&db).await.unwrap();
            }
        }
        let control =
            pontia_agent_control::AgentControl::builder(db, root.path().to_path_buf()).build();
        let app = AppState::builder(control)
            .external_api_token(Some("test-token".into()))
            .workspace_browser(WorkspaceBrowserConfig {
                roots: vec![WorkspaceRootConfig {
                    root_id: "projects".into(),
                    label: "Projects".into(),
                    path: projects.display().to_string(),
                }],
            })
            .build();
        let mut state = HttpState::from(app.clone());
        if bound {
            state = state.with_bot_identity(Arc::new(Sessions));
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router(state)).await.unwrap();
        });
        Self {
            root,
            app,
            addr,
            server,
        }
    }
    async fn cli(&self, session: &str, args: &[&str]) -> Output {
        let addr = self.addr.to_string();
        let root = self.root.path().to_path_buf();
        let session = session.to_owned();
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        tokio::task::spawn_blocking(move || {
            Command::new(env!("CARGO_BIN_EXE_pontia"))
                .arg("bot")
                .args(args)
                .env("PONTIA_BOT_API_ADDR", addr)
                .env("PONTIA_HOME", root)
                .env("PONTIA_BOT_TOOL_TOKEN", &session)
                .env("PONTIA_SESSION_ID", session)
                .output()
                .unwrap()
        })
        .await
        .unwrap()
    }
    async fn call(&self, args: &[&str]) -> Value {
        let mut args = args.to_vec();
        args.push("--json");
        let output = self.cli("bot", &args).await;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
    async fn http(&self, method: reqwest::Method, path: &str, input: Value) -> reqwest::Response {
        reqwest::Client::new()
            .request(method, format!("http://{}{path}", self.addr))
            .bearer_auth("test-token")
            .json(&input)
            .send()
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
async fn user_cli_updates_shared_file_and_returns_current_text_only_on_rejection() {
    let f = Fixture::new(true).await;
    let result = f
        .call(&[
            "update-user",
            "--old-text",
            "",
            "--new-text",
            "语言：中文\n",
        ])
        .await;
    assert_eq!(result, json!({"updated":true}));
    let plain = f
        .cli(
            "bot",
            &["update-user", "--old-text", "中文", "--new-text", "英文"],
        )
        .await;
    assert!(plain.status.success());
    assert!(!plain.stdout.is_empty());
    assert!(serde_json::from_slice::<Value>(&plain.stdout).is_err());
    let path = f.root.path().join("bot/USER.md");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "语言：英文\n");
    let failed = f
        .cli(
            "bot",
            &["update-user", "--old-text", "中文", "--new-text", "法文"],
        )
        .await;
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("语言：英文"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "语言：英文\n");
    for session in ["helper", "dreaming", "unknown"] {
        assert!(
            !f.cli(
                session,
                &["update-user", "--old-text", "", "--new-text", "bad"]
            )
            .await
            .status
            .success()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "语言：英文\n");
    }
}

#[tokio::test]
async fn workspace_tools_reuse_canonical_identity_and_persist_descriptions_for_control_queries() {
    let f = Fixture::new(true).await;
    let first = f
        .call(&[
            "workspace",
            "register",
            "--root-id",
            "projects",
            "--path",
            "app",
            "--description",
            "用户的项目",
        ])
        .await;
    let id = first["workspace"]["workspace_id"].as_str().unwrap();
    let again = f
        .call(&[
            "workspace",
            "register",
            "--root-id",
            "projects",
            "--path",
            "./alias",
        ])
        .await;
    assert_eq!(again["workspace"]["workspace_id"], id);
    assert_eq!(again["workspace"]["description"], "用户的项目");
    let limit = "文🙂".repeat(1_000);
    let changed = f
        .call(&[
            "workspace",
            "describe",
            "--workspace-id",
            id,
            "--description",
            &limit,
        ])
        .await;
    assert_eq!(changed["workspace"]["workspace_id"], id);
    assert_eq!(changed["workspace"]["description"], limit);
    assert_eq!(
        changed["workspace"]["last_used_at"],
        again["workspace"]["last_used_at"]
    );
    // Reopen the file database to verify the description is durable and shared with Agent Control.
    let reopened = pontia_storage_sqlite::connect_sqlite(&format!(
        "sqlite://{}",
        f.root.path().join("test.db").display()
    ))
    .await
    .unwrap();
    let queries = ControlQueryService::new(reopened.clone());
    assert_eq!(
        queries
            .get_workspace(id)
            .await
            .unwrap()
            .unwrap()
            .description
            .as_deref(),
        Some(limit.as_str())
    );
    let listed = queries.list_workspaces().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].description.as_deref(), Some(limit.as_str()));
    assert_eq!(
        pontia_agent_control::get_workspace_record(&reopened, id)
            .await
            .unwrap()
            .unwrap()
            .description
            .as_deref(),
        Some(limit.as_str())
    );
    let response = f
        .http(
            reqwest::Method::GET,
            &format!("/api/v1/workspaces/{id}"),
            Value::Null,
        )
        .await;
    assert_eq!(
        response.json::<Value>().await.unwrap()["data"]["workspace"]["description"],
        limit
    );
    let failed = f
        .cli(
            "bot",
            &[
                "workspace",
                "describe",
                "--workspace-id",
                id,
                "--description",
                &"x".repeat(2_001),
            ],
        )
        .await;
    assert!(!failed.status.success());
    assert_eq!(
        queries
            .get_workspace(id)
            .await
            .unwrap()
            .unwrap()
            .description
            .as_deref(),
        Some(limit.as_str())
    );
    let plain = f
        .cli(
            "bot",
            &[
                "workspace",
                "describe",
                "--workspace-id",
                id,
                "--clear-description",
            ],
        )
        .await;
    assert!(plain.status.success());
    assert!(serde_json::from_slice::<Value>(&plain.stdout).is_err());
    let text = String::from_utf8(plain.stdout).unwrap();
    assert!(text.contains(id));
    assert!(text.contains(&f.root.path().join("projects/app").display().to_string()));
    assert_eq!(
        queries
            .get_workspace(id)
            .await
            .unwrap()
            .unwrap()
            .description,
        None
    );
}

#[tokio::test]
async fn registration_keeps_directory_scope_and_explicit_activation_boundary() {
    let f = Fixture::new(true).await;
    for path in ["missing", "file", "../", "/tmp", "escape"] {
        assert!(
            !f.cli(
                "bot",
                &[
                    "workspace",
                    "register",
                    "--root-id",
                    "projects",
                    "--path",
                    path
                ]
            )
            .await
            .status
            .success()
        );
    }
    assert!(!f.root.path().join("projects/missing").exists());
    let queries = f.app.control().queries();
    assert!(queries.list_workspaces().await.unwrap().is_empty());
    assert!(
        !f.cli(
            "bot",
            &[
                "workspace",
                "register",
                "--root-id",
                "projects",
                "--path",
                "app",
                "--description",
                &"x".repeat(2_001)
            ]
        )
        .await
        .status
        .success()
    );
    assert!(queries.list_workspaces().await.unwrap().is_empty());
    let first = f
        .call(&[
            "workspace",
            "register",
            "--root-id",
            "projects",
            "--path",
            "app",
        ])
        .await;
    let id = first["workspace"]["workspace_id"].as_str().unwrap();
    assert!(
        f.http(
            reqwest::Method::DELETE,
            &format!("/api/v1/workspaces/{id}"),
            Value::Null
        )
        .await
        .status()
        .is_success()
    );
    f.call(&[
        "workspace",
        "describe",
        "--workspace-id",
        id,
        "--description",
        "保持归档状态",
    ])
    .await;
    assert_eq!(
        queries.get_workspace(id).await.unwrap().unwrap().state,
        "deleted"
    );
    assert!(queries.list_workspaces().await.unwrap().is_empty());
    for session in ["helper", "dreaming"] {
        assert!(
            !f.cli(
                session,
                &[
                    "workspace",
                    "register",
                    "--root-id",
                    "projects",
                    "--path",
                    "app"
                ]
            )
            .await
            .status
            .success()
        );
        assert!(
            !f.cli(
                session,
                &[
                    "workspace",
                    "describe",
                    "--workspace-id",
                    id,
                    "--description",
                    "bad"
                ]
            )
            .await
            .status
            .success()
        );
    }
    assert_eq!(
        queries.get_workspace(id).await.unwrap().unwrap().state,
        "deleted"
    );
    let again = f
        .call(&[
            "workspace",
            "register",
            "--root-id",
            "projects",
            "--path",
            "alias",
        ])
        .await;
    assert_eq!(again["workspace"]["workspace_id"], id);
    assert_eq!(again["workspace"]["state"], "active");
    assert_eq!(again["workspace"]["description"], "保持归档状态");
    assert!(
        !f.cli(
            "bot",
            &[
                "workspace",
                "describe",
                "--workspace-id",
                "missing",
                "--description",
                "bad"
            ]
        )
        .await
        .status
        .success()
    );
    assert_eq!(queries.list_workspaces().await.unwrap().len(), 1);
}

#[tokio::test]
async fn production_tools_reject_unbound_sessions_forged_roles_and_missing_authentication() {
    let f = Fixture::new(false).await;
    for (path, request) in [
        (
            "/api/v1/bot/user",
            json!({"session_id":"bot", "input":{"old_text":"", "new_text":"secret"}}),
        ),
        (
            "/api/v1/bot/workspace",
            json!({"session_id":"bot", "command":{"action":"register", "input":{"root_id":"projects", "path":"app"}}}),
        ),
    ] {
        assert_eq!(
            f.http(reqwest::Method::POST, path, request.clone())
                .await
                .status(),
            403
        );
        let response = reqwest::Client::new()
            .post(format!("http://{}{path}", f.addr))
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
        let mut forged = request;
        forged["role"] = json!("bot");
        assert_eq!(
            f.http(reqwest::Method::POST, path, forged).await.status(),
            400
        );
    }
    assert!(!f.root.path().join("bot/USER.md").exists());
    assert!(
        f.app
            .control()
            .queries()
            .list_workspaces()
            .await
            .unwrap()
            .is_empty()
    );
}
