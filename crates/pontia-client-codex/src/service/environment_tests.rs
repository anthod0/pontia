use crate::runtime::{CodexRuntime, protocol::Notification};
use pontia_agent_control::{AgentControl, CreateSessionRequest, clients::ClientRegistry};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};
use tokio::sync::broadcast;

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn daemon(root: &Path) -> Process {
    let socket = root.join("app-server.sock");
    let process = Process(
        Command::new("codex")
            .args(["app-server", "--listen"])
            .arg(format!("unix://{}", socket.display()))
            .env("CODEX_HOME", root)
            .env("PONTIA_SESSION_ID", "wrong-parent")
            .env("PROBE_DROP", "drop")
            .current_dir(root)
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(root.join("daemon.log")).unwrap())
            .spawn()
            .unwrap(),
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if socket.exists()
                && CodexRuntime::install_for_test(root, socket.clone())
                    .await
                    .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    process
}

async fn command_result(
    events: &mut broadcast::Receiver<Notification>,
    session: &str,
    filtered: bool,
) {
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut command = None;
        loop {
            let event = events.recv().await.unwrap();
            if event["method"] == "item/completed"
                && event["params"]["item"]["type"] == "commandExecution"
            {
                let item = &event["params"]["item"];
                assert_eq!(item["exitCode"], 0, "{item}");
                command = Some(
                    serde_json::from_str::<Value>(
                        item["aggregatedOutput"].as_str().unwrap().trim(),
                    )
                    .unwrap(),
                );
            }
            if event["method"] == "turn/completed" {
                assert_eq!(event["params"]["turn"]["status"], "completed", "{event:?}");
                assert_eq!(
                    command.unwrap(),
                    json!({
                        "PONTIA_SESSION_ID":session,
                        "PROBE_KEEP":"keep ' quotes \\ and \n newline",
                        "PROBE_NATIVE":"native",
                        "PROBE_DROP":if filtered { Value::Null } else { json!("drop") }
                    })
                );
                break;
            }
        }
    })
    .await
    .unwrap();
}

async fn probe(runtime: &CodexRuntime, thread: &str, session: &str, filtered: bool) {
    let connection = runtime.connection().await.unwrap();
    let mut events = connection.events.subscribe();
    connection
        .call(
            "turn/start",
            json!({"threadId":thread,"input":[{"type":"text","text":"Read the environment."}]}),
        )
        .await
        .unwrap();
    command_result(&mut events, session, filtered).await;
}

#[tokio::test]
#[ignore = "requires installed codex and python3; uses an isolated local model provider"]
async fn real_codex_commands_keep_identity_across_start_and_resume() {
    let inherited = std::env::var_os("PONTIA_SESSION_ID");
    for (policy, filtered) in [
        ("", false),
        ("include_only = []", false),
        ("exclude = [\"PROBE_DROP\"]", true),
        ("filters = { PROBE_DROP = \"exclude\" }", true),
        (
            "include_only = [\"PATH\", \"HOME\", \"PROBE_KEEP\", \"PROBE_NATIVE\"]",
            true,
        ),
        (
            "filters = { PATH = \"include\", HOME = \"include\", PROBE_KEEP = \"include\", PROBE_NATIVE = \"include\", PROBE_DROP = \"exclude\" }",
            true,
        ),
    ] {
        // Subprocesses are stopped before this directory is dropped.
        let root = tempfile::tempdir().unwrap();
        let mut provider = Process(
            Command::new("python3")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/environment_provider.py"
                ))
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let mut port = String::new();
        BufReader::new(provider.0.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        std::fs::write(
            root.path().join("config.toml"),
            format!(
                r#"
model_provider = "probe"
model = "probe-model"
approval_policy = "on-request"
sandbox_mode = "workspace-write"
[model_providers.probe]
name = "Local environment fixture"
base_url = "http://127.0.0.1:{}/v1"
wire_api = "responses"
requires_openai_auth = false
[projects.{}]
trust_level = "trusted"
[shell_environment_policy]
set = {{ PROBE_NATIVE = "native", PONTIA_SESSION_ID = "wrong-native" }}
{policy}
"#,
                port.trim(),
                toml::Value::String(root.path().display().to_string())
            ),
        )
        .unwrap();
        let mut server = daemon(root.path()).await;
        let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
            .await
            .unwrap();
        pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
        pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
            pool.clone(),
        )
        .save("codex", "default", Some(true), None)
        .await
        .unwrap();
        let mut clients = ClientRegistry::default();
        clients.register(crate::registration());
        let app = AgentControl::builder(pool, root.path().into())
            .clients(clients)
            .build();
        let mut sessions = Vec::new();
        for _ in 0..2 {
            let mut request: CreateSessionRequest =
                serde_json::from_value(json!({"client_type":"codex","workspace":root.path()}))
                    .unwrap();
            request.runtime_environment.extend([
                ("PONTIA_SESSION_ID".into(), "wrong-request".into()),
                (
                    "PROBE_KEEP".into(),
                    "keep ' quotes \\ and \n newline".into(),
                ),
            ]);
            let created = app
                .session_commands()
                .create_session(request)
                .await
                .unwrap();
            let session = created.session_id().unwrap().to_owned();
            let runtime = CodexRuntime::existing(root.path()).await.unwrap();
            let mut events = runtime.connection().await.unwrap().events.subscribe();
            app.turn_commands()
                .create_and_dispatch_turn(&session, "Read the environment.".into(), json!({}))
                .await
                .unwrap();
            command_result(&mut events, &session, filtered).await;
            let binding = pontia_agent_control::AgentBindingService::new(app.db())
                .binding_for_session(&session)
                .await
                .unwrap()
                .unwrap();
            sessions.push((session, binding.client_session_key));
        }
        for (session, _) in &sessions {
            app.session_commands()
                .terminate_session(session)
                .await
                .unwrap();
        }
        CodexRuntime::shutdown(root.path()).await;
        drop(server);
        server = daemon(root.path()).await;
        for (session, thread) in &sessions {
            app.session_commands()
                .resume_session(session, root.path())
                .await
                .unwrap();
            probe(
                &CodexRuntime::existing(root.path()).await.unwrap(),
                thread,
                session,
                filtered,
            )
            .await;
            app.session_commands()
                .terminate_session(session)
                .await
                .unwrap();
        }
        app.inbox_commands().stop_scheduling().await;
        CodexRuntime::shutdown(root.path()).await;
        drop(server);
    }
    assert_eq!(std::env::var_os("PONTIA_SESSION_ID"), inherited);
}
