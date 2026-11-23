use std::{
    collections::HashMap,
    process::{Child, Command},
};

use pontia::shutdown::Shutdown;
use pontia_runtime::ProcessFingerprint;
use tempfile::tempdir;

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
        let mut unrelated = Sleeper::start();
        let mut stale = unrelated.fingerprint();
        stale.start_time_seconds -= 1;

        for (session, client, role, fp) in [
            ("owned", "acp", "acp", owned.fingerprint()),
            ("stale", "claude-agent", "claude_sdk_host", stale),
            ("native", "pi", "pi_tui", unrelated.fingerprint()),
        ] {
            sqlx::query("INSERT INTO sessions(session_id,client_type,state,title,workspace_ref) VALUES(?,?,'idle',?,'/workspace')").bind(session).bind(client).bind(session).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO session_runtimes(runtime_id,session_id,role,state,process_fingerprint,created_at) VALUES(?,?,?,'running',?,'2026-01-01')").bind(session).bind(session).bind(role).bind(serde_json::to_string(&fp).unwrap()).execute(&pool).await.unwrap();
        }

        let shutdown = Shutdown::inspect(&config).await.unwrap();
        let error = shutdown
            .run(operation, false, || -> Result<(), String> {
                panic!("service cleanup ran despite live agents")
            })
            .await
            .unwrap_err();
        assert!(error.contains(&format!("pontia {operation} --force")));
        assert!(error.contains("\"owned\" (acp, /workspace)"));
        assert!(!error.contains("stale") && !error.contains("native"));
        assert!(owned.0.try_wait().unwrap().is_none());
        Shutdown::inspect(&config)
            .await
            .unwrap()
            .run(operation, true, || {
                assert!(owned.0.try_wait().unwrap().is_some());
                assert!(unrelated.0.try_wait().unwrap().is_none());
                Ok(())
            })
            .await
            .unwrap();
        let states: (String,String) = sqlx::query_as("SELECT s.state,r.state FROM sessions s JOIN session_runtimes r USING(session_id) WHERE session_id='owned'").fetch_one(&pool).await.unwrap();
        assert_eq!(states, ("exited".into(), "exited".into()));
        let stopped = Shutdown::inspect(&config)
            .await
            .unwrap()
            .run(operation, false, || Ok(true))
            .await
            .unwrap();
        assert!(stopped);
    }
}

#[tokio::test]
async fn unverified_background_listener_requires_force_and_survives_cleanup() {
    let root = tempdir().unwrap();
    let config = pontia_clients::config_from_vars(&HashMap::from([(
        "PONTIA_HOME".into(),
        root.path().display().to_string(),
    )]))
    .unwrap();
    let directory = root.path().join("state/codex");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("background-app-server.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let shutdown = Shutdown::inspect(&config).await.unwrap();
    assert!(
        shutdown
            .require_force("down", false)
            .unwrap_err()
            .contains(path.to_str().unwrap())
    );
    assert!(pontia_runtime::launch_lock::launch(root.path()).is_err());
    shutdown.run("down", true, || Ok(())).await.unwrap();
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
