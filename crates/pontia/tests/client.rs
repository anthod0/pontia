use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn entry(client: &str, key: &str, id: &str, short: &str) -> Value {
    json!({"id":id,"client_type":client,"client_key":key,"name":"Editable display name","short_id":short,"enabled":true,"has_record":false,"config_overrides":{},"effective_config":{},"created_at":null,"updated_at":null})
}

fn run(
    args: &[&str],
    entries: Vec<Value>,
    expected: Option<(&str, &str, Option<Value>)>,
) -> std::process::Output {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("overrides.json"),
        r#"{"tui_command":"database-pi"}"#,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    std::fs::write(
        root.path().join("config.toml"),
        format!("bind_addr = \"{addr}\"\nexternal_api_token = \"test-token\"\n"),
    )
    .unwrap();
    let expected =
        expected.map(|(method, path, body)| (method.to_string(), path.to_string(), body));
    let (finished, child_finished) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let mut requests = vec![(
            "GET".to_string(),
            "/api/v1/agent-clients".to_string(),
            None,
            json!({"agent_clients":entries}),
        )];
        if let Some((method, path, body)) = expected {
            requests.push((method, path, body, json!({"saved":true})));
        }
        for (method, path, expected_body, response) in requests {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("missing request: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert_ne!(count, 0);
                bytes.extend_from_slice(&buffer[..count]);
                let text = String::from_utf8_lossy(&bytes);
                if let Some((headers, body)) = text.split_once("\r\n\r\n") {
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(str::to_owned)
                        })
                        .map(|length| length.parse().unwrap())
                        .unwrap_or(0);
                    if body.len() < length {
                        continue;
                    }
                    assert_eq!(
                        headers.lines().next().unwrap(),
                        format!("{method} {path} HTTP/1.1")
                    );
                    assert!(
                        headers
                            .to_ascii_lowercase()
                            .contains("authorization: bearer test-token")
                    );
                    if let Some(expected) = expected_body {
                        assert_eq!(serde_json::from_str::<Value>(body).unwrap(), expected);
                    }
                    break;
                }
            }
            let body = json!({"data":response,"meta":{},"error":null}).to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        // Keep accepting until the CLI has exited; no scheduling window may hide a mutation.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(listener.accept().is_err(), "unexpected additional request");
            if child_finished.try_recv().is_ok() {
                break;
            }
            assert!(Instant::now() < deadline, "CLI did not exit");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(listener.accept().is_err(), "request queued before CLI exit");
    });
    let output = Command::new(env!("CARGO_BIN_EXE_pontia"))
        .arg("client")
        .args(args)
        .current_dir(root.path())
        .env("PONTIA_HOME", root.path())
        .env("PONTIA_PI_TUI_COMMAND", "caller-only-command")
        .output()
        .unwrap();
    let _ = finished.send(());
    server.join().unwrap();
    output
}

#[test]
fn single_short_selector_and_opaque_ids_locate_without_display_names() {
    for selector in [vec!["show", "kimi"], vec!["show", "--id", "opaque-key"]] {
        let output = run(
            &selector,
            vec![entry("acp", "kimi", "opaque-key", "kimi")],
            Some(("GET", "/api/v1/agent-clients/acp/kimi", None)),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"saved":true})
        );
    }
}

#[test]
fn ambiguous_and_unknown_selectors_never_mutate() {
    for selector in ["pi", "missing", "Editable display name"] {
        let output = run(
            &["disable", selector],
            vec![
                entry("pi", "default", "native", "pi"),
                entry("acp", "pi", "peer", "pi"),
            ],
            None,
        );
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    for (id, path) in [
        ("native", "/api/v1/agent-clients/pi/default"),
        ("peer", "/api/v1/agent-clients/acp/pi"),
    ] {
        let output = run(
            &["disable", "--id", id],
            vec![
                entry("pi", "default", "native", "pi"),
                entry("acp", "pi", "peer", "pi"),
            ],
            Some(("PATCH", path, Some(json!({"enabled":false})))),
        );
        assert!(output.status.success());
    }
}

#[test]
fn management_commands_send_only_the_requested_database_fields() {
    for (args, method, body) in [
        (vec!["enable", "pi"], "PATCH", Some(json!({"enabled":true}))),
        (
            vec!["disable", "pi"],
            "PATCH",
            Some(json!({"enabled":false})),
        ),
        (
            vec!["set", "pi", "--config", "overrides.json"],
            "PATCH",
            Some(json!({"config_overrides":{"tui_command":"database-pi"}})),
        ),
        (vec!["reset", "pi"], "DELETE", None),
    ] {
        assert!(
            run(
                &args,
                vec![entry("pi", "default", "opaque", "pi")],
                Some((method, "/api/v1/agent-clients/pi/default", body))
            )
            .status
            .success()
        );
    }
}

#[test]
fn missing_or_conflicting_target_selectors_are_rejected() {
    for args in [vec!["show"], vec!["show", "pi", "--id", "opaque"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_pontia"))
            .arg("client")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
