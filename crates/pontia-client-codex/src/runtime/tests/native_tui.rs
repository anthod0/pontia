use super::tui::{TuiCleanup, app, session};
use super::*;

// The caller supplies an externally started, isolated native daemon via CODEX_HOME.
#[tokio::test]
#[ignore = "requires an isolated Codex 0.156.1 daemon, credentials and model access"]
async fn native_tui_restart_recovers_idle_busy_and_approval() {
    let root = tempfile::tempdir().unwrap();
    let _tuis = TuiCleanup(root.path().into());
    let runtime = CodexRuntime::ensure(root.path()).await.unwrap();
    let direct = Connection::connect(&runtime.socket_path).await.unwrap();
    let app = app(root.path()).await;
    let service = crate::CodexService::new(app.event_ingest_service());
    let mut sessions = Vec::new();
    let mut threads = Vec::new();
    for _ in 0..3 {
        let created = direct.call("thread/start", json!({"cwd":root.path(),"historyMode":"legacy","approvalPolicy":"on-request","sandbox":"workspace-write"})).await.unwrap();
        let thread = created["thread"]["id"].as_str().unwrap().to_string();
        direct.call("turn/start", json!({"threadId":thread,"input":[{"type":"text","text":"Reply READY. Do not use tools."}]})).await.unwrap();
        tokio::time::timeout(Duration::from_secs(90), async {
            loop {
                let turns = direct
                    .call("thread/turns/list", json!({"threadId":thread,"limit":20}))
                    .await
                    .unwrap();
                if turns["data"]
                    .as_array()
                    .is_some_and(|turns| turns.len() == 1 && turns[0]["status"] == "completed")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .unwrap();
        let id = session(&app, &runtime, &thread).await;
        service.open_tui(&id).await.unwrap();
        sessions.push(id);
        threads.push(thread);
    }
    let (tmux_socket, _) = runtime.tui_pane(&sessions[0]).unwrap();
    let direct_command = format!(
        "exec env CODEX_HOME='{}' codex resume --remote 'unix://{}' '{}'",
        runtime.connection.codex_home.display(),
        runtime.socket_path.display(),
        threads[1]
    );
    let output = std::process::Command::new("tmux")
        .args([
            "-S",
            &tmux_socket,
            "new-session",
            "-d",
            "-P",
            "-F",
            "#{pane_id}",
            "-s",
            "direct-native",
            "-c",
            &root.path().to_string_lossy(),
            &direct_command,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let direct_pane = String::from_utf8_lossy(&output.stdout).trim().to_string();
    tokio::time::sleep(Duration::from_secs(1)).await;
    let direct_fingerprint = pontia_runtime::GenericRuntimeManager
        .capture_tmux_process_fingerprint(&tmux_socket, &direct_pane, &["codex"])
        .unwrap();
    let busy_file = root.path().join("busy.log");
    let release_file = root.path().join("release-busy");
    let approval_file = root.path().join("approval.log");
    direct.call("turn/start", json!({"threadId":threads[1],"input":[{"type":"text","text":format!("Use exec_command to run exactly once: while [ ! -f {} ]; do sleep 0.2; done; printf 'BUSY_ONCE\\n' >> {} . Wait for completion, then reply DONE. Do not run any other commands.",release_file.display(),busy_file.display())}]})).await.unwrap();
    let mut requests = direct.events.subscribe();
    direct.call("turn/start", json!({"threadId":threads[2],"input":[{"type":"text","text":format!("Use exec_command exactly once with sandbox_permissions=require_escalated, justification='Verify recovery of a pending approval', and command: printf 'APPROVAL_ONCE\\n' >> {} . Wait for the user's approval. Do not run without escalation, do not retry, and then reply DONE.",approval_file.display())}]})).await.unwrap();
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let event = requests.recv().await.unwrap();
            if event["method"] == "item/commandExecution/requestApproval"
                && event["params"]["threadId"] == threads[2]
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(!approval_file.exists());
    let original: Vec<super::super::tui::TuiProcess> = sessions
        .iter()
        .map(|id| runtime.saved_tui(id, "process").unwrap().unwrap())
        .collect();
    CodexRuntime::shutdown(root.path()).await;
    tokio::time::timeout(Duration::from_secs(125), async {
        loop {
            if original.iter().all(|process| {
                let output = std::process::Command::new("tmux")
                    .args([
                        "-S",
                        &process.socket,
                        "capture-pane",
                        "-p",
                        "-t",
                        &process.pane,
                    ])
                    .output()
                    .unwrap();
                String::from_utf8_lossy(&output.stdout)
                    .contains("app-server session could not be restored")
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(original.iter().all(|process| process.is_alive()));
    assert!(direct.is_connected());
    assert!(!approval_file.exists());
    let replacement = CodexRuntime::ensure(root.path()).await.unwrap();
    assert_eq!(replacement.instance_id, runtime.instance_id);
    for id in &sessions {
        replacement.gateway(id).await.unwrap();
    }
    assert!(service.open_tui(&sessions[0]).await.is_err());
    assert!(original.iter().all(|process| process.is_alive()));
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_secs(25)).await;
        eprintln!("native TUI recovery: preserving the full reconnect window");
    }
    for (id, thread) in sessions.iter().zip(&threads) {
        service.open_tui(id).await.unwrap();
        assert_eq!(
            replacement.saved_target(id).unwrap().unwrap().thread["id"],
            *thread
        );
    }
    assert!(original.iter().all(|process| !process.is_alive()));
    let busy_turns = direct
        .call(
            "thread/turns/list",
            json!({"threadId":threads[1],"limit":20}),
        )
        .await
        .unwrap();
    assert!(
        busy_turns["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|turn| turn["status"] == "inProgress"),
        "background execution must survive TUI replacement"
    );
    assert!(!busy_file.exists());
    std::fs::write(&release_file, "release").unwrap();
    let (socket, pane) = replacement.tui_pane(&sessions[2]).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        std::process::Command::new("tmux")
            .args(["-S", &socket, "send-keys", "-t", &pane, "Enter"])
            .status()
            .unwrap()
            .success()
    );
    let (socket, pane) = replacement.tui_pane(&sessions[0]).unwrap();
    assert!(
        std::process::Command::new("tmux")
            .args([
                "-S",
                &socket,
                "send-keys",
                "-t",
                &pane,
                "-l",
                "Reply exactly IDLE_RECOVERED. Do not use tools."
            ])
            .status()
            .unwrap()
            .success()
    );
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(
        std::process::Command::new("tmux")
            .args(["-S", &socket, "send-keys", "-t", &pane, "Enter"])
            .status()
            .unwrap()
            .success()
    );
    for thread in &threads {
        tokio::time::timeout(Duration::from_secs(90), async {
            loop {
                let turns = direct
                    .call("thread/turns/list", json!({"threadId":thread,"limit":20}))
                    .await
                    .unwrap();
                let turns = turns["data"].as_array().unwrap();
                assert!(turns.len() <= 2, "recovery must not replay a Turn");
                if turns.len() == 2 && turns.iter().all(|turn| turn["status"] == "completed") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        })
        .await
        .unwrap();
    }
    assert_eq!(std::fs::read_to_string(busy_file).unwrap(), "BUSY_ONCE\n");
    assert_eq!(
        std::fs::read_to_string(approval_file).unwrap(),
        "APPROVAL_ONCE\n"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM turns")
            .fetch_one(&app.db())
            .await
            .unwrap(),
        0,
        "TUI recovery alone cannot synthesize Turn facts"
    );
    assert!(
        pontia_runtime::GenericRuntimeManager.validate_tmux_process_fingerprint(
            &tmux_socket,
            &direct_pane,
            &direct_fingerprint
        )
    );
    CodexRuntime::shutdown(root.path()).await;
    assert!(direct.is_connected());
}
