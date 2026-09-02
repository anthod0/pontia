use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    thread,
};

fn pontia() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pontia"))
}

fn temp_dir(_name: &str) -> tempfile::TempDir {
    tempfile::tempdir().expect("create temp dir")
}

fn write_pontia_config(home: &Path, port: u16) {
    fs::write(
        home.join("config.toml"),
        format!("bind_addr = \"0.0.0.0:{port}\"\nexternal_api_token = \"cli-test-token\"\n"),
    )
    .expect("write pontia config");
}

fn install_fake_tmux(bin_dir: &Path) {
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
    capture_one_request_with_response(listener, r#"{"data":{"submitted":true}}"#)
}

fn capture_one_request_with_response(
    listener: TcpListener,
    response_body: &'static str,
) -> thread::JoinHandle<String> {
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
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        )
        .expect("write response");
        String::from_utf8(bytes).expect("request is utf-8")
    })
}

#[test]
fn rejects_a_missing_command() {
    let output = pontia().output().expect("run pontia");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Usage: pontia <COMMAND>"));
}

#[test]
fn help_describes_the_cli() {
    let output = pontia().arg("--help").output().expect("run pontia");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is utf-8");
    assert!(stdout.contains("Control Pontia from the command line"));
    assert!(stdout.contains("Usage: pontia"));
    assert!(stdout.contains("init"));
    assert!(stdout.contains("Configure Pontia interactively"));
}

#[test]
fn version_reports_the_workspace_version() {
    let output = pontia().arg("--version").output().expect("run pontia");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("version is utf-8"),
        format!("pontia {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn workflow_commands_reject_invalid_pontia_home_before_reading_inputs() {
    for (name, pontia_home) in [
        ("empty", ""),
        ("relative", "relative/pontia"),
        ("tilde", "~/.pontia"),
    ] {
        let mut command = pontia();
        command
            .args(["workflow", "run", "/input/must-not-be-read.toml"])
            .env("PONTIA_HOME", pontia_home);
        let output = command.output().expect("run pontia");
        assert!(!output.status.success(), "{name}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.starts_with("pontia: "), "{name}: {stderr}");
        assert!(stderr.contains("PONTIA_HOME"), "{name}: {stderr}");
        assert!(!stderr.contains("Workflow file"), "{name}: {stderr}");
    }
}

#[test]
fn workflow_run_posts_a_linear_agent_workflow_from_toml() {
    let dir = temp_dir("run-success");
    let definition = dir.path().join("workflow.toml");
    fs::write(dir.path().join("requirements.md"), "Build Workflow run.\n").expect("write handoff");
    fs::write(
        &definition,
        r#"title = "Implement Workflow run"
cwd = "."

[[handoffs]]
name = "requirements.md"
source = "./requirements.md"

[[nodes]]
type = "agent"
phase = "Discovery"
title = "Research"
instructions = "Research the implementation."
inputs = ["requirements.md"]
output = "research.md"
execution_profile_id = "researcher"
execution_profile_version = "1"

[[nodes]]
type = "agent"
phase = "Delivery"
title = "Implement"
instructions = "Implement the feature."
inputs = ["research.md"]
output = "result.md"
"#,
    )
    .expect("write workflow definition");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server address");
    write_pontia_config(dir.path(), addr.port());
    let request = capture_one_request_with_response(
        listener,
        r#"{"data":{"workflow_id":"wf_created","node_id":"node_root","session_id":"sess_root"}}"#,
    );

    let output = pontia()
        .args(["workflow", "run", definition.to_str().expect("utf-8 path")])
        .env("PONTIA_HOME", dir.path())
        .output()
        .expect("run workflow");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout"),
        "wf_created\n"
    );
    let request = request.join().expect("request capture thread");
    assert!(request.starts_with("POST /internal/v1/workflows HTTP/1.1"));
    assert!(request.contains("authorization: Bearer cli-test-token"));
    let (_, body) = request.split_once("\r\n\r\n").expect("request body");
    let body: serde_json::Value = serde_json::from_str(body).expect("JSON request");
    assert!(
        body["workflow_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("wf_"))
    );
    assert_eq!(body["title"], "Implement Workflow run");
    assert_eq!(
        body["cwd"],
        dir.path()
            .canonicalize()
            .expect("canonical dir")
            .display()
            .to_string()
    );
    assert_eq!(
        body["handoffs"],
        serde_json::json!([{"name":"requirements.md", "content":"Build Workflow run.\n"}])
    );
    assert_eq!(body["nodes"][0]["type"], "agent");
    assert_eq!(body["nodes"][0]["phase"], "Discovery");
    assert_eq!(body["nodes"][1]["phase"], "Delivery");
    assert_eq!(body["nodes"][0]["execution_profile_id"], "researcher");
    assert_eq!(
        body["nodes"][1]["inputs"],
        serde_json::json!(["research.md"])
    );
}

#[test]
fn workflow_run_rejects_missing_phase_and_unknown_fields_in_toml() {
    let dir = temp_dir("run-invalid-definition");
    for (name, extra, expected) in [
        ("missing-phase", "", "missing field `phase`"),
        (
            "unknown-field",
            "phase = \"Build\"\npriority = 1\n",
            "unknown field `priority`",
        ),
    ] {
        let definition = dir.path().join(format!("{name}.toml"));
        fs::write(
            &definition,
            format!(
                "title = \"Invalid\"\ncwd = \".\"\n\n[[nodes]]\ntype = \"agent\"\n{extra}title = \"Worker\"\ninstructions = \"Work.\"\noutput = \"result.md\"\n"
            ),
        )
        .expect("write invalid workflow definition");

        let output = pontia()
            .args(["workflow", "run", definition.to_str().expect("utf-8 path")])
            .env("PONTIA_HOME", dir.path())
            .output()
            .expect("run workflow");

        assert!(!output.status.success(), "{name}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn workflow_show_prints_compact_agent_readable_context() {
    let dir = temp_dir("show-success");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server address");
    write_pontia_config(dir.path(), addr.port());
    let request = capture_one_request_with_response(
        listener,
        r#"{"data":{"context":{"workflow":{"workflow_id":"wf_show","title":"Ship release","state":"running","failure_message":null,"agent_submitted_count":1,"agent_total_count":3,"current_node_id":"node_build","nodes":[{"node_id":"node_plan","phase":"Plan","title":"Plan release","status":"submitted"},{"node_id":"node_build","phase":"Build","title":"Build release","status":"running"},{"node_id":"node_review","phase":"Review","title":"Review release","status":"pending"}]},"current_node":{"instructions":"Build the release.","inputs":[{"name":"plan.md","content":"Use the compact plan.\n"}],"output":"result.md"}}}}"#,
    );

    let output = pontia()
        .args(["workflow", "show", "wf_show"])
        .env("PONTIA_HOME", dir.path())
        .output()
        .expect("run workflow show");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout");
    assert!(stdout.contains("# Ship release"));
    assert!(stdout.contains("Workflow: `wf_show` | State: running | Progress: 1/3"));
    assert!(stdout.contains("## Current node: Build — Build release"));
    assert!(stdout.contains("### Instructions\nBuild the release."));
    assert!(stdout.contains("#### `plan.md`\nUse the compact plan."));
    assert!(stdout.contains("- ✓ Plan — Plan release (submitted)"));
    assert!(stdout.contains("- → Build — Build release (running)"));
    assert!(stdout.contains("- · Review — Review release (pending)"));

    let request = request.join().expect("request capture thread");
    assert!(request.starts_with("GET /external/v1/workflows/wf_show/context HTTP/1.1"));
    assert!(request.contains("authorization: Bearer cli-test-token"));
}

#[test]
fn workflow_show_uses_workflow_id_from_environment_when_argument_is_omitted() {
    let dir = temp_dir("show-environment");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server address");
    write_pontia_config(dir.path(), addr.port());
    let request = capture_one_request_with_response(
        listener,
        r#"{"data":{"context":{"workflow":{"workflow_id":"wf_environment","title":"Environment workflow","state":"running","failure_message":null,"agent_submitted_count":0,"agent_total_count":1,"current_node_id":"node_current","nodes":[{"node_id":"node_current","phase":"Build","title":"Build","status":"running"}]},"current_node":{"instructions":"Build.","inputs":[],"output":"result.md"}}}}"#,
    );

    let output = pontia()
        .args(["workflow", "show"])
        .env("PONTIA_HOME", dir.path())
        .env("PONTIA_WORKFLOW_ID", "wf_environment")
        .output()
        .expect("run workflow show");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = request.join().expect("request capture thread");
    assert!(request.starts_with("GET /external/v1/workflows/wf_environment/context HTTP/1.1"));
}

#[test]
fn workflow_show_requires_an_id_when_argument_and_environment_are_missing() {
    let dir = temp_dir("show-missing-id");
    write_pontia_config(dir.path(), 9);

    let output = pontia()
        .args(["workflow", "show"])
        .env("PONTIA_HOME", dir.path())
        .env_remove("PONTIA_WORKFLOW_ID")
        .output()
        .expect("run workflow show");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("WORKFLOW_ID is required when PONTIA_WORKFLOW_ID is not set")
    );
}

#[test]
fn workflow_submit_discovers_managed_pane_and_posts_utf8_handoff() {
    let dir = temp_dir("submit-success");
    let bin_dir = dir.path().join("bin");
    fs::create_dir(&bin_dir).expect("create bin dir");
    install_fake_tmux(&bin_dir);
    let input = dir.path().join("handoff.txt");
    fs::write(&input, "Workflow result: 完成\n").expect("write input");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server address");
    write_pontia_config(dir.path(), addr.port());
    let request = capture_one_request(listener);
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = pontia()
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
        .env("PONTIA_HOME", dir.path())
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
}

#[test]
fn workflow_patch_request_reads_utf8_before_discovery_and_prints_patch_id() {
    let dir = temp_dir("patch-request-success");
    let bin_dir = dir.path().join("bin");
    fs::create_dir(&bin_dir).expect("create bin dir");
    install_fake_tmux(&bin_dir);
    let input = dir.path().join("request.md");
    fs::write(&input, "Revise the remaining plan. 完成\n").expect("write request");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("test server address");
    write_pontia_config(dir.path(), addr.port());
    let request = capture_one_request_with_response(
        listener,
        r#"{"data":{"patch_id":"patch_cli_1","state":"requested"}}"#,
    );
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = pontia()
        .args([
            "workflow",
            "patch",
            "request",
            "--input",
            input.to_str().expect("UTF-8 path"),
        ])
        .env("PATH", path)
        .env("TMUX", "/tmp/tmux-test/default,1,0")
        .env("TMUX_PANE", "%7")
        .env("PONTIA_HOME", dir.path())
        .output()
        .expect("run Workflow Patch request");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout"),
        "patch_cli_1\n"
    );
    let request = request.join().expect("request capture thread");
    assert!(request.starts_with("POST /internal/v1/workflow/patches/request HTTP/1.1"));
    assert!(request.contains(
        r#"{"session_id":"sess_workflow_cli","runtime_instance_id":"rtinst_workflow_cli","document":"Revise the remaining plan. 完成\n"}"#
    ));
}

#[test]
fn workflow_patch_block_reads_reason_before_discovery_and_uses_managed_identity() {
    let dir = temp_dir("patch-block-success");
    let bin_dir = dir.path().join("bin");
    fs::create_dir(&bin_dir).expect("create bin dir");
    install_fake_tmux(&bin_dir);
    let reason = dir.path().join("reason.md");
    fs::write(&reason, "No executable continuation. 完成\n").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    write_pontia_config(dir.path(), addr.port());
    let request = capture_one_request_with_response(
        listener,
        r#"{"data":{"patch_id":"patch_cli_1","workflow_id":"wf_cli","state":"blocked"}}"#,
    );
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = pontia()
        .args([
            "workflow",
            "patch",
            "block",
            "--reason",
            reason.to_str().unwrap(),
        ])
        .env("PATH", path)
        .env("TMUX", "/tmp/tmux-test/default,1,0")
        .env("TMUX_PANE", "%7")
        .env("PONTIA_HOME", dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "blocked\n");
    let request = request.join().unwrap();
    assert!(request.starts_with("POST /internal/v1/workflow/patches/block HTTP/1.1"));
    assert!(request.contains(
        r#"{"session_id":"sess_workflow_cli","runtime_instance_id":"rtinst_workflow_cli","reason":"No executable continuation. 完成\n"}"#
    ));

    let invalid = dir.path().join("invalid-reason.md");
    fs::write(&invalid, [0xff, 0xfe]).unwrap();
    let output = pontia()
        .args([
            "workflow",
            "patch",
            "block",
            "--reason",
            invalid.to_str().unwrap(),
        ])
        .env("PONTIA_HOME", dir.path())
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to read UTF-8 Workflow Patch reason file"));
    assert!(!stderr.contains("not running in a Pontia-managed tmux pane"));
}

#[test]
fn workflow_patch_request_rejects_local_input_before_pane_discovery() {
    let dir = temp_dir("patch-request-source-errors");
    let invalid = dir.path().join("invalid.md");
    fs::write(&invalid, [0xff, 0xfe]).expect("write invalid UTF-8");

    for input in [dir.path().join("missing.md"), invalid] {
        let output = pontia()
            .args([
                "workflow",
                "patch",
                "request",
                "--input",
                input.to_str().expect("UTF-8 path"),
            ])
            .env("PONTIA_HOME", dir.path())
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .output()
            .expect("run Workflow Patch request");
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("failed to read UTF-8 Workflow Patch request file"),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr)
                .contains("not running in a Pontia-managed tmux pane")
        );
    }
}

#[test]
fn workflow_submit_rejects_missing_and_non_utf8_input_before_pane_discovery() {
    let dir = temp_dir("source-errors");
    let missing = dir.path().join("missing.txt");
    let invalid = dir.path().join("invalid.txt");
    fs::write(&invalid, [0xff, 0xfe]).expect("write invalid utf-8");

    for (input, expected) in [
        (missing, "failed to read UTF-8 input file"),
        (invalid, "stream did not contain valid UTF-8"),
    ] {
        let output = pontia()
            .args([
                "workflow",
                "submit",
                "--input",
                input.to_str().expect("utf-8 input path"),
                "--output",
                "result.md",
            ])
            .env("PONTIA_HOME", dir.path())
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
}

#[test]
fn workflow_submit_rejects_a_pane_without_pontia_identity() {
    let dir = temp_dir("unmanaged-pane");
    let bin_dir = dir.path().join("bin");
    fs::create_dir(&bin_dir).expect("create bin dir");
    let tmux = bin_dir.join("tmux");
    fs::write(&tmux, "#!/bin/sh\nexit 1\n").expect("write fake tmux");
    let mut permissions = fs::metadata(&tmux)
        .expect("fake tmux metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(tmux, permissions).expect("make fake tmux executable");
    let input = dir.path().join("handoff.txt");
    fs::write(&input, "result").expect("write input");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = pontia()
        .args([
            "workflow",
            "submit",
            "--input",
            input.to_str().expect("utf-8 input path"),
            "--output",
            "result.md",
        ])
        .env("PATH", path)
        .env("PONTIA_HOME", dir.path())
        .env("TMUX", "/tmp/tmux-test/default,1,0")
        .env("TMUX_PANE", "%8")
        .output()
        .expect("run workflow submit");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("not running in a Pontia-managed tmux pane")
    );
}
