use pontia_application::{
    AppState, CreateSessionRequest, client_contract::ClientServicePhase, clients::ClientRegistry,
};
use pontia_runtime::local_service::{DefinitionStore, ProcessCommandRunner};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use std::{collections::HashMap, path::Path, time::Duration};
use tokio::io::AsyncBufReadExt;

struct NoDefinitions;
impl DefinitionStore for NoDefinitions {
    fn read(&self, _: &Path) -> Result<Option<String>, String> {
        panic!("No Claude service")
    }
    fn install(&self, _: &Path, _: &str) -> Result<bool, String> {
        panic!("No Claude service")
    }
}

#[tokio::test]
#[ignore = "build with PONTIA_CLAUDE_BUNDLE, then run --test delivery -- --ignored"]
async fn bundled_install_reports_authentication_retries_without_fabricating_turns() {
    let root = tempfile::tempdir().unwrap();
    let pontia = root.path().join("pontia");
    let config = root.path().join("claude");
    std::fs::create_dir(&config).unwrap();
    let vars = HashMap::from([
        ("PONTIA_HOME".into(), pontia.to_str().unwrap().into()),
        ("CLAUDE_CONFIG_DIR".into(), config.to_str().unwrap().into()),
    ]);
    let setup = pontia_client_claude::setup::integration()
        .prepare(&vars, root.path(), &ProcessCommandRunner)
        .unwrap();
    setup.preflight(&ProcessCommandRunner).unwrap();
    assert!(
        !pontia.exists(),
        "preflight must not install or initialize a conversation"
    );
    setup
        .install(&ProcessCommandRunner, &NoDefinitions)
        .unwrap();
    assert!(std::fs::read_dir(&config).unwrap().next().is_none());

    let mut server = tokio::process::Command::new("node")
        .args([
            "--input-type=module",
            "-e",
            r#"
            import { createServer } from 'node:http';
            const server = createServer(async (request, response) => {
                for await (const chunk of request) {}
                response.writeHead(401, { 'content-type': 'application/json' });
                response.end(JSON.stringify({ type: 'error', error: {
                    type: 'authentication_error', message: 'fixture credential rejected'
                }}));
            });
            server.listen(0, '127.0.0.1', () => console.log(server.address().port));
        "#,
        ])
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut port = String::new();
    tokio::io::BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut port)
        .await
        .unwrap();
    let address = format!("127.0.0.1:{}", port.trim().parse::<u16>().unwrap());
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_claude::registration_with_host(None));
    let app = AppState::builder(pool, pontia)
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
    let mut request: CreateSessionRequest = serde_json::from_value(json!({"client_type":"claude-agent","workspace":root.path(),"initial_task":{"input":"Reply with a test marker"}})).unwrap();
    request.runtime_environment = [
        ("CLAUDE_CONFIG_DIR".into(), config.to_str().unwrap().into()),
        ("ANTHROPIC_BASE_URL".into(), format!("http://{address}")),
        (
            "ANTHROPIC_API_KEY".into(),
            "fixture-secret-must-not-leak".into(),
        ),
        ("ANTHROPIC_AUTH_TOKEN".into(), "".into()),
        (
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
            "1".into(),
        ),
        ("DISABLE_AUTOUPDATER".into(), "1".into()),
    ]
    .into();
    let session = app
        .session_commands()
        .create_session(request)
        .await
        .unwrap()
        .session_id()
        .unwrap()
        .to_owned();
    let diagnostic = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let view =
                serde_json::to_value(app.queries().get_session(&session).await.unwrap().unwrap())
                    .unwrap();
            if view["claude-agent"]["unavailable_reason"] == "authentication_failed_retrying" {
                break view;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id=?")
        .bind(&session)
        .fetch_one(&app.db())
        .await
        .unwrap();
    let closed = app.session_commands().terminate_session(&session).await;
    let after_close =
        serde_json::to_value(app.queries().get_session(&session).await.unwrap().unwrap()).unwrap();
    app.shutdown().notify();
    tasks.join().await.unwrap();
    server.kill().await.unwrap();
    let diagnostic = diagnostic.unwrap();
    assert!(
        !diagnostic
            .to_string()
            .contains("fixture-secret-must-not-leak")
    );
    assert_eq!(turns, 0);
    closed.unwrap();
    assert_eq!(after_close["state"], "exited");
    assert!(after_close["claude-agent"]["unavailable_reason"].is_null());
    let events: Vec<String> = sqlx::query_scalar("SELECT payload FROM events WHERE session_id=?")
        .bind(session)
        .fetch_all(&app.db())
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .all(|event| !event.contains("fixture-secret-must-not-leak"))
    );
}
