use std::{os::unix::fs::PermissionsExt, path::Path, process::Stdio, sync::Arc, time::Duration};

use pontia_runtime::pi_control::{PiControlConnection, PiControlEndpoint};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
    process::Command,
};

fn endpoint(path: &Path, runtime_instance_id: &str) -> PiControlEndpoint {
    PiControlEndpoint {
        runtime_instance_id: runtime_instance_id.into(),
        socket_path: path.display().to_string(),
        version: 1,
    }
}

#[tokio::test]
async fn rust_controls_actual_pi_extension_and_recovers_after_controller_restart() {
    let root = tempfile::Builder::new()
        .prefix("pc-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../clients/pi/test/control-server.mjs");
    let mut child = Command::new("node")
        .arg(fixture)
        .arg(root.path())
        .args(["sess_pi", "rtinst_pi"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("Node 24 is required for the Pi cross-language test");
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut path = String::new();
    tokio::time::timeout(Duration::from_secs(10), output.read_line(&mut path))
        .await
        .unwrap()
        .unwrap();
    assert!(!path.is_empty(), "Pi listener must publish its socket path");
    let endpoint = endpoint(Path::new(path.trim()), "rtinst_pi");
    let connection =
        Arc::new(PiControlConnection::new("sess_pi".into(), endpoint.clone()).unwrap());
    let (first, concurrent) = tokio::join!(connection.ping(), connection.ping());
    first.unwrap();
    concurrent.unwrap();
    connection
        .submit("hello\n你好", Some("msg_one"))
        .await
        .unwrap();
    let message: Value =
        serde_json::from_str(&std::fs::read_to_string(root.path().join("messages.jsonl")).unwrap())
            .unwrap();
    assert_eq!(
        message,
        json!({"input":"hello\n你好", "inboxMessageId":"msg_one"})
    );
    std::fs::remove_file(root.path().join("messages.jsonl")).unwrap();
    let second = PiControlConnection::new("sess_pi".into(), endpoint.clone()).unwrap();
    assert!(
        second
            .ping()
            .await
            .unwrap_err()
            .to_string()
            .contains("connection_busy")
    );
    connection.ping().await.unwrap();
    connection.invalidate();
    assert!(connection.ping().await.is_err());
    // The peer processes FIN asynchronously; each retry here is a fresh ping invocation.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if second.ping().await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    second.invalidate();
    child.stdin.take();
    assert!(child.wait().await.unwrap().success());
    assert!(!Path::new(path.trim()).exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn timeout_disconnect_and_malformed_replies_fail_without_replaying_requests() {
    for failure in [
        "timeout",
        "disconnect",
        "wrong_id",
        "invalid_result",
        "oversized",
        "wrong_identity",
    ] {
        let root = tempfile::Builder::new()
            .prefix("pc-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let path = root.path().join("s");
        let listener = UnixListener::bind(&path).unwrap();
        let connection = Arc::new(
            PiControlConnection::new("sess_pi".into(), endpoint(&path, "rtinst_pi")).unwrap(),
        );
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(socket);
            let mut line = String::new();
            stream.read_line(&mut line).await.unwrap();
            let hello: Value = serde_json::from_str(&line).unwrap();
            let identity = if failure == "wrong_identity" {
                "rtinst_other"
            } else {
                "rtinst_pi"
            };
            let reply = json!({"version": 1, "request_id": hello["request_id"], "result": {"session_id": "sess_pi", "runtime_instance_id": identity}});
            // Deliberately split the response across writes.
            let encoded = format!("{reply}\n");
            stream
                .get_mut()
                .write_all(&encoded.as_bytes()[..9])
                .await
                .unwrap();
            tokio::task::yield_now().await;
            stream
                .get_mut()
                .write_all(&encoded.as_bytes()[9..])
                .await
                .unwrap();
            if failure == "wrong_identity" {
                return;
            }
            line.clear();
            stream.read_line(&mut line).await.unwrap();
            let ping: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(ping["method"], "submit");
            assert_eq!(ping["input"], "exactly once");
            match failure {
                "disconnect" => return,
                "wrong_id" => {
                    stream.get_mut().write_all(b"{\"version\":1,\"request_id\":\"other\",\"result\":{\"pong\":true}}\n").await.unwrap();
                }
                "invalid_result" => {
                    let reply = json!({"version":1,"request_id":ping["request_id"],"result":{"accepted":false}});
                    stream
                        .get_mut()
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .unwrap();
                }
                "oversized" => {
                    let _ = stream.get_mut().write_all(&vec![b'a'; 65537]).await;
                }
                "timeout" => {}
                _ => unreachable!(),
            }
            line.clear();
            assert_eq!(
                stream.read_line(&mut line).await.unwrap(),
                0,
                "failed connection must close without another request"
            );
            assert!(
                tokio::time::timeout(Duration::from_millis(50), listener.accept())
                    .await
                    .is_err(),
                "failed requests must not reconnect themselves"
            );
        });
        let error = connection.submit("exactly once", None).await.unwrap_err();
        assert_eq!(
            matches!(error, pontia_core::Error::ControlUnknown(_)),
            failure != "wrong_identity",
            "{failure}: {error}"
        );
        let error = error.to_string();
        let expected = match failure {
            "timeout" => "timed out",
            "disconnect" => "closed",
            "wrong_id" => "request_id mismatch",
            "oversized" => "64 KiB",
            "invalid_result" => "invalid Pi control submit response",
            "wrong_identity" => "identity mismatch",
            _ => unreachable!(),
        };
        assert!(error.contains(expected), "{failure}: {error}");
        server.await.unwrap();
    }
}

#[tokio::test]
async fn invalidation_ends_an_in_flight_request() {
    let root = tempfile::Builder::new()
        .prefix("pc-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = root.path().join("s");
    let listener = UnixListener::bind(&path).unwrap();
    let connection =
        Arc::new(PiControlConnection::new("sess_pi".into(), endpoint(&path, "rtinst_pi")).unwrap());
    let caller = {
        let connection = connection.clone();
        tokio::spawn(async move { connection.ping().await })
    };
    let (socket, _) = listener.accept().await.unwrap();
    let mut stream = BufReader::new(socket);
    let mut line = String::new();
    stream.read_line(&mut line).await.unwrap();
    connection.invalidate();
    assert!(
        tokio::time::timeout(Duration::from_millis(250), caller)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("binding has changed")
    );
    line.clear();
    assert_eq!(stream.read_line(&mut line).await.unwrap(), 0);
}
