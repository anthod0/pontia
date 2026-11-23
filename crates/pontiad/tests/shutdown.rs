use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
};

use pontia_runtime::ProcessFingerprint;
use serde_json::{Value, json};
use tempfile::tempdir;

fn oneshot(root: &Path, operation: &str, force: bool, stop: bool) -> Result<Vec<Value>, String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pontiad"))
        .arg("shutdown")
        .env("PONTIA_HOME", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"pontia_home":root,"operation": operation, "force": force, "stop": stop});
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let out = child.wait_with_output().unwrap();
    if !out.status.success() {
        return Err(String::from_utf8(out.stderr).unwrap());
    }
    Ok(String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect())
}

struct Sleeper(Child);

impl Sleeper {
    fn start() -> Self {
        Self(Command::new("sleep").arg("120").spawn().unwrap())
    }
    fn fingerprint(&self) -> ProcessFingerprint {
        ProcessFingerprint::capture(self.0.id()).unwrap()
    }
}
impl Drop for Sleeper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn shutdown_only_stops_owned_processes_and_records_exit_before_service_cleanup() {
    for operation in ["down", "uninstall"] {
        let root = tempdir().unwrap();
        let config = pontia_clients::config_from_vars(&HashMap::from([(
            "PONTIA_HOME".into(),
            root.path().display().to_string(),
        )]))
        .unwrap();
        let pool = pontia_storage_sqlite::connect_sqlite(&config.database_url)
            .await
            .unwrap();
        pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
        let mut owned = Sleeper::start();
        let mut managed_pi = Sleeper::start();
        let mut background = Sleeper::start();
        let mut unrelated = Sleeper::start();
        let mut stale = unrelated.fingerprint();
        stale.start_time_seconds -= 1;

        for (session, client, role, fp) in [
            ("owned", "acp", "acp", owned.fingerprint()),
            ("managed", "pi", "pi_tui", managed_pi.fingerprint()),
            ("other-role", "acp", "other", unrelated.fingerprint()),
            ("stale", "claude-agent", "claude_sdk_host", stale),
            ("native", "pi", "pi_tui", unrelated.fingerprint()),
        ] {
            sqlx::query("INSERT INTO sessions(session_id,client_type,state,title,workspace_ref) VALUES(?,?,'idle',?,'/workspace')").bind(session).bind(client).bind(session).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO session_runtimes(runtime_id,session_id,role,state,process_fingerprint,created_at) VALUES(?,?,?,'running',?,'2026-01-01T00:00:00Z')").bind(session).bind(session).bind(role).bind(serde_json::to_string(&fp).unwrap()).execute(&pool).await.unwrap();
        }

        for (session, source, kind) in [
            ("managed", "external_api", "session.resuming"),
            ("native", "external_api", "session.starting"),
            ("native", "agent_plugin", "session.resuming"),
        ] {
            sqlx::query("INSERT INTO events(event_id,session_id,source,client_type,event_type,occurred_at) VALUES(?,?,?,'pi',?,'2026-01-01T00:00:00Z')")
                .bind(format!("{session}-{kind}")).bind(session).bind(source).bind(kind).execute(&pool).await.unwrap();
        }
        let directory = root.path().join("state/codex");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("background-app-server.owner"),
            serde_json::to_vec(&[background.fingerprint()]).unwrap(),
        )
        .unwrap();
        for mode in ["background", "foreground"] {
            sqlx::query("INSERT INTO sessions(session_id,client_type,state,title) VALUES(?,'codex','idle',?)")
                .bind(mode).bind(mode).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO events(event_id,session_id,source,client_type,event_type,occurred_at,payload) VALUES(?,?,'external_api','codex','session.created','2026-01-01T00:00:00Z',?)")
                .bind(mode).bind(mode).bind(json!({"launch_mode":mode}).to_string()).execute(&pool).await.unwrap();
        }
        let error = oneshot(root.path(), operation, false, true).unwrap_err();
        assert!(error.contains(&format!("pontia {operation} --force")));
        assert!(error.contains("\"owned\" (acp, /workspace)"));
        assert!(error.contains("\"managed\" (pi, /workspace)"));
        assert!(error.contains("\"background\" (codex, No Workspace)"));
        assert!(error.contains(&format!(
            "Codex background app-server (PID {})",
            background.0.id()
        )));
        for skipped in ["stale", "native", "other-role", "foreground"] {
            assert!(!error.contains(skipped));
        }
        assert!(owned.0.try_wait().unwrap().is_none());
        let stopped = oneshot(root.path(), operation, true, true).unwrap();
        assert_eq!(stopped[1], json!({"stopped": []}));
        assert!(owned.0.try_wait().unwrap().is_some());
        assert!(managed_pi.0.try_wait().unwrap().is_some());
        assert!(background.0.try_wait().unwrap().is_some());
        assert!(unrelated.0.try_wait().unwrap().is_none());
        let modes: Vec<String> = sqlx::query_scalar(
            "SELECT state FROM sessions WHERE client_type='codex' ORDER BY session_id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(modes, ["exited", "idle"]);
        let runtime_exits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id='managed' AND event_type='runtime.exited'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(runtime_exits, 1);
        let states: (String,String) = sqlx::query_as("SELECT s.state,r.state FROM sessions s JOIN session_runtimes r USING(session_id) WHERE session_id='owned'").fetch_one(&pool).await.unwrap();
        assert_eq!(states, ("exited".into(), "exited".into()));
        assert_eq!(
            oneshot(root.path(), operation, false, true).unwrap(),
            vec![json!({"listing":[]}), json!({"stopped":[]})]
        );
    }
}

#[tokio::test]
async fn unverified_background_listener_requires_force_and_survives_cleanup() {
    let root = tempdir().unwrap();
    let occupied_port = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    std::fs::write(
        root.path().join("config.toml"),
        format!("bind_addr = '{}'\n", occupied_port.local_addr().unwrap()),
    )
    .unwrap();
    let directory = root.path().join("state/codex");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("background-app-server.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let lock = pontia_runtime::launch_lock::launch(root.path()).unwrap();
    assert!(
        oneshot(root.path(), "down", true, true)
            .unwrap_err()
            .contains("in progress")
    );
    drop(lock);
    assert!(
        oneshot(root.path(), "down", false, true)
            .unwrap_err()
            .contains(path.to_str().unwrap())
    );
    let listing = oneshot(root.path(), "uninstall", true, false).unwrap();
    assert_eq!(listing.len(), 1);
    assert!(
        listing[0]["listing"][0]
            .as_str()
            .unwrap()
            .contains("ownership unverified")
    );
    let stopped = oneshot(root.path(), "down", true, true).unwrap();
    assert!(
        stopped[1]["stopped"][0]
            .as_str()
            .unwrap()
            .contains(path.to_str().unwrap())
    );
    std::os::unix::net::UnixStream::connect(&path).unwrap();
    assert!(pontia_runtime::launch_lock::launch(root.path()).is_ok());
    assert!(!root.path().join("data").exists());
}

#[test]
fn a_reused_tmux_pane_number_and_command_do_not_prove_ownership() {
    let root = tempdir().unwrap();
    let socket = root.path().join("tmux.sock");
    let socket = socket.to_str().unwrap();
    let launch = "launch_evt_01234567-89ab-cdef";
    let command = format!("exec sleep 90 # /state/launch/{launch}.sh");
    let tmux = |args: &[&str]| {
        Command::new("tmux")
            .args(["-S", socket, "-f", "/dev/null"])
            .args(args)
            .output()
            .unwrap()
    };
    let output = tmux(&["new-session", "-d", "-s", "unrelated", &command]);
    assert!(output.status.success());
    struct Server<'a>(&'a str);
    impl Drop for Server<'_> {
        fn drop(&mut self) {
            let _ = Command::new("tmux")
                .args(["-S", self.0, "kill-server"])
                .status();
        }
    }
    let _server = Server(socket);
    let log = root.path().join("runtime.log");
    std::fs::write(
        &log,
        format!("session=test launch={launch} pontia runtime started\n"),
    )
    .unwrap();
    let panes = pontia_runtime::pane_processes(socket).unwrap();
    let error = panes["%0"]
        .owner("runtime", None, &log, "test")
        .unwrap_err();
    assert!(error.to_string().contains("cannot verify"));
    assert!(tmux(&["has-session", "-t", "unrelated"]).status.success());
}
