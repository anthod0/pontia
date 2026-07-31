use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

fn pontiactl() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pontiactl"))
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("pontiactl-{name}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn install_fake_tmux(bin_dir: &PathBuf) {
    let tmux = bin_dir.join("tmux");
    fs::write(
        &tmux,
        r#"#!/bin/sh
case "$*" in
  *"@pontia_session_id") printf 'sess_workflow_cli\n' ;;
  *"@pontia_runtime_instance_id") printf 'rtinst_workflow_cli\n' ;;
  *) exit 1 ;;
esac
"#,
    )
    .expect("write fake tmux");
    let mut permissions = fs::metadata(&tmux)
        .expect("fake tmux metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(tmux, permissions).expect("make fake tmux executable");
}

fn capture_one_request(listener: TcpListener) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let count = stream.read(&mut buffer).expect("read request");
            if count == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..count]);
            let text = String::from_utf8_lossy(&bytes);
            let Some((headers, body)) = text.split_once("\r\n\r\n") else {
                continue;
            };
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if body.len() >= content_length {
                break;
            }
        }
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 27\r\nConnection: close\r\n\r\n{\"data\":{\"submitted\":true}}",
            )
            .expect("write response");
        String::from_utf8(bytes).expect("request is utf-8")
    })
}

#[test]
fn starts_without_a_command() {
    let output = pontiactl().output().expect("run pontiactl");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn help_describes_the_cli() {
    let output = pontiactl().arg("--help").output().expect("run pontiactl");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is utf-8");
    assert!(stdout.contains("Control Pontia from the command line"));
    assert!(stdout.contains("Usage: pontiactl"));
}

#[test]
fn version_reports_the_workspace_version() {
    let output = pontiactl()
        .arg("--version")
        .output()
        .expect("run pontiactl");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("version is utf-8"),
        format!("pontiactl {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn workflow_submit_discovers_managed_pane_and_posts_utf8_handoff() {
    let dir = temp_dir("submit-success");
    let bin_dir = dir.join("bin");
    fs::create_dir(&bin_dir).expect("create bin dir");
    install_fake_tmux(&bin_dir);
    let input = dir.join("handoff.txt");
    fs::write(&input, "Workflow result: 完成\n").expect("write input");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server address");
    let request = capture_one_request(listener);
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = pontiactl()
        .args([
            "workflow",
            "submit",
            "--input",
            input.to_str().expect("utf-8 input path"),
            "--output",
            "result.md",
        ])
        .env("PATH", path)
        .env("TMUX", "/tmp/tmux-test/default,1,0")
        .env("TMUX_PANE", "%7")
        .env("PONTIA_BIND_ADDR", format!("0.0.0.0:{}", addr.port()))
        .env("PONTIA_EXTERNAL_API_TOKEN", "cli-test-token")
        .output()
        .expect("run workflow submit");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = request.join().expect("request capture thread");
    assert!(request.starts_with("POST /internal/v1/workflow/submissions HTTP/1.1"));
    assert!(request.contains(&format!("host: 127.0.0.1:{}", addr.port())));
    assert!(request.contains("authorization: Bearer cli-test-token"));
    assert!(request.contains(
        r#"{"session_id":"sess_workflow_cli","runtime_instance_id":"rtinst_workflow_cli","output":"result.md","content":"Workflow result: 完成\n"}"#
    ));

    fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn workflow_submit_rejects_missing_and_non_utf8_input_before_pane_discovery() {
    let dir = temp_dir("source-errors");
    let missing = dir.join("missing.txt");
    let invalid = dir.join("invalid.txt");
    fs::write(&invalid, [0xff, 0xfe]).expect("write invalid utf-8");

    for (input, expected) in [
        (missing, "failed to read UTF-8 input file"),
        (invalid, "stream did not contain valid UTF-8"),
    ] {
        let output = pontiactl()
            .args([
                "workflow",
                "submit",
                "--input",
                input.to_str().expect("utf-8 input path"),
                "--output",
                "result.md",
            ])
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .output()
            .expect("run workflow submit");

        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn workflow_submit_rejects_a_pane_without_pontia_identity() {
    let dir = temp_dir("unmanaged-pane");
    let bin_dir = dir.join("bin");
    fs::create_dir(&bin_dir).expect("create bin dir");
    let tmux = bin_dir.join("tmux");
    fs::write(&tmux, "#!/bin/sh\nexit 1\n").expect("write fake tmux");
    let mut permissions = fs::metadata(&tmux)
        .expect("fake tmux metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(tmux, permissions).expect("make fake tmux executable");
    let input = dir.join("handoff.txt");
    fs::write(&input, "result").expect("write input");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = pontiactl()
        .args([
            "workflow",
            "submit",
            "--input",
            input.to_str().expect("utf-8 input path"),
            "--output",
            "result.md",
        ])
        .env("PATH", path)
        .env("TMUX", "/tmp/tmux-test/default,1,0")
        .env("TMUX_PANE", "%8")
        .output()
        .expect("run workflow submit");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("not running in a Pontia-managed tmux pane")
    );

    fs::remove_dir_all(dir).expect("remove temp dir");
}
