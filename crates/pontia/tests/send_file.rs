use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
};

#[test]
fn send_file_requires_session_identity_without_contacting_service() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pontia"))
        .args(["send-file", "report.txt"])
        .env("PONTIA_HOME", root.path())
        .env_remove("PONTIA_SESSION_ID")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("PONTIA_SESSION_ID"));
}

#[test]
fn send_file_resolves_path_and_prints_the_returned_file_id() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    std::fs::write(
        root.path().join("config.toml"),
        format!(
            "bind_addr = \"{}\"\nexternal_api_token = \"test-token\"\n",
            listener.local_addr().unwrap()
        ),
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let header_end;
        loop {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            bytes.push(byte[0]);
            if bytes.ends_with(b"\r\n\r\n") {
                header_end = bytes.len();
                break;
            }
        }
        let headers = String::from_utf8(bytes.clone()).unwrap().to_lowercase();
        assert!(headers.starts_with("post /api/v1/sessions/session-test/files http/1.1"));
        assert!(headers.contains("authorization: bearer test-token"));
        let size = headers
            .lines()
            .find_map(|s| s.strip_prefix("content-length: "))
            .unwrap()
            .parse::<usize>()
            .unwrap();
        bytes.resize(header_end + size, 0);
        socket.read_exact(&mut bytes[header_end..]).unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes[header_end..]).unwrap();
        let response = r#"{"data":{"file":{"file_id":"file-result"}},"error":null}"#;
        write!(socket, "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
        body
    });
    let output = Command::new(env!("CARGO_BIN_EXE_pontia"))
        .args(["send-file", "report.txt"])
        .current_dir(root.path())
        .env("PONTIA_HOME", root.path())
        .env("PONTIA_SESSION_ID", "session-test")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "file-result"
    );
    assert_eq!(
        server.join().unwrap(),
        serde_json::json!({"path":root.path().join("report.txt")})
    );
}
