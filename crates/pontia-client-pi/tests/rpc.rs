use pontia_agent_control::{
    ClientControlChannel, DeliveryError, InputPayload, ipc::rpc::MAX_FRAME_BYTES,
};
use pontia_client_pi::rpc::{MAX_CONTROL_FRAME_BYTES, PiRpcPeer};
use pontia_core::Error;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

#[tokio::test]
async fn serves_reverse_requests_while_awaiting_a_response() {
    let (left, right) = UnixStream::pair().unwrap();
    let (daemon, mut incoming) = PiRpcPeer::new(left);
    let task = {
        let daemon = daemon.clone();
        tokio::spawn(async move { daemon.call("submit", json!({"input":"你好"})).await })
    };
    let mut pi = BufReader::new(right);
    let mut line = String::new();
    pi.read_line(&mut line).await.unwrap();
    let submit: Value = serde_json::from_str(&line).unwrap();
    pi.get_mut()
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":\"pi:1\",\"method\":\"session.context\",\"params\":{}}\n",
        )
        .await
        .unwrap();
    let request = incoming.recv().await.unwrap();
    assert_eq!(request.method, "session.context");
    daemon
        .reply(request.id, json!({"session_context":null}))
        .await
        .unwrap();
    line.clear();
    pi.read_line(&mut line).await.unwrap();
    let context: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(context["id"], "pi:1");
    assert!(!task.is_finished());
    pi.get_mut()
        .write_all(
            format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":submit["id"],"result":{"accepted":true}})
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap(), json!({"accepted":true}));
    daemon.close();
}

#[tokio::test]
async fn correlates_concurrent_calls_with_out_of_order_responses() {
    let (left, right) = UnixStream::pair().unwrap();
    let (peer, _incoming) = PiRpcPeer::new(left);
    let server = tokio::spawn(async move {
        let mut stream = BufReader::new(right);
        let mut requests = Vec::new();
        for _ in 0..2 {
            let mut line = String::new();
            stream.read_line(&mut line).await.unwrap();
            requests.push(serde_json::from_str::<Value>(&line).unwrap());
        }
        for request in requests.into_iter().rev() {
            stream
                .get_mut()
                .write_all(
                    format!(
                        "{}\n",
                        json!({"jsonrpc":"2.0","id":request["id"],"result":request["method"]})
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let (a, b) = tokio::join!(peer.call("one", json!({})), peer.call("two", json!({})));
    assert_eq!(a.unwrap(), "one");
    assert_eq!(b.unwrap(), "two");
    server.await.unwrap();
    peer.close();
}

#[tokio::test]
async fn reasoning_effort_waits_for_the_pi_acknowledgement() {
    let (left, right) = UnixStream::pair().unwrap();
    let (peer, _incoming) = PiRpcPeer::new(left);
    let (received, wait_until_received) = tokio::sync::oneshot::channel();
    let (release, wait_until_released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let mut pi = BufReader::new(right);
        let mut line = String::new();
        pi.read_line(&mut line).await.unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["method"], "set_reasoning_effort");
        assert_eq!(request["params"], json!({"reasoning_effort":"max"}));
        received.send(()).unwrap();
        wait_until_released.await.unwrap();
        pi.get_mut()
            .write_all(
                format!(
                    "{}\n",
                    json!({"jsonrpc":"2.0","id":request["id"],"result":{"accepted":true}})
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    let pending = tokio::spawn({
        let peer = peer.clone();
        async move {
            pontia_agent_control::ClientControlChannel::set_reasoning_effort(peer.as_ref(), "max")
                .await
        }
    });
    wait_until_received.await.unwrap();
    assert!(!pending.is_finished());
    release.send(()).unwrap();
    pending.await.unwrap().unwrap();
    server.await.unwrap();
    peer.close();
}

#[tokio::test]
async fn a_lost_response_is_unknown_and_is_never_replayed() {
    let (left, right) = UnixStream::pair().unwrap();
    let (peer, _incoming) = PiRpcPeer::new(left);
    let server = tokio::spawn(async move {
        let mut stream = BufReader::new(right);
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["method"], "submit");
    });
    assert!(matches!(
        peer.call("submit", json!({"input":"one"})).await,
        Err(Error::ControlUnknown(_))
    ));
    assert!(matches!(
        peer.call("submit", json!({"input":"two"})).await,
        Err(Error::CapabilityUnavailable(_))
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn malformed_responses_close_the_connection_without_accepting_delivery() {
    for response in [
        json!({"jsonrpc":"1.0","id":"pontia:0","result":{}}),
        json!({"jsonrpc":"2.0","id":"wrong","result":{}}),
        json!({"jsonrpc":"2.0","id":"pontia:0","result":{},"error":{"code":1,"message":"bad"}}),
        json!({"jsonrpc":"2.0","id":"pontia:0","error":{"code":"bad","message":"bad"}}),
    ] {
        let (left, right) = UnixStream::pair().unwrap();
        let (peer, _incoming) = PiRpcPeer::new(left);
        let server = tokio::spawn(async move {
            let mut stream = BufReader::new(right);
            let mut line = String::new();
            stream.read_line(&mut line).await.unwrap();
            stream
                .get_mut()
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        });
        assert!(matches!(
            peer.call("submit", json!({})).await,
            Err(Error::ControlUnknown(_))
        ));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn oversized_frames_are_rejected_and_cancellation_releases_pending_calls() {
    let (left, mut right) = UnixStream::pair().unwrap();
    let (peer, _incoming) = PiRpcPeer::new(left);
    assert!(matches!(
        peer.call(
            "submit",
            json!({"input":"x".repeat(MAX_CONTROL_FRAME_BYTES)})
        )
        .await,
        Err(Error::Domain(_))
    ));
    assert!(matches!(
        peer.call("event.report", json!({"data":"x".repeat(MAX_FRAME_BYTES)}))
            .await,
        Err(Error::Domain(_))
    ));
    let task = {
        let peer = peer.clone();
        tokio::spawn(async move { peer.call("ping", json!({})).await })
    };
    let mut stream = BufReader::new(&mut right);
    let mut line = String::new();
    stream.read_line(&mut line).await.unwrap();
    task.abort();
    let _ = task.await;
    assert!(peer.is_closed());
}

#[tokio::test]
async fn busy_rejection_is_distinct_from_uncertain_delivery() {
    for code in [-32010, -32007] {
        let (left, right) = UnixStream::pair().unwrap();
        let (peer, _) = PiRpcPeer::new(left);
        let server = tokio::spawn(async move {
            let mut pi = BufReader::new(right);
            let mut line = String::new();
            pi.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            pi.get_mut().write_all(format!("{}\n", json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":code,"message":"delivery diagnostic"}})).as_bytes()).await.unwrap();
        });
        let error = peer
            .call("submit", json!({"input":"once"}))
            .await
            .unwrap_err();
        if code == -32010 {
            assert!(matches!(
                error,
                Error::Conflict {
                    code: "input_busy",
                    ..
                }
            ));
        } else {
            assert!(matches!(error, Error::ControlUnknown(_)));
        }
        server.await.unwrap();
        peer.close();
    }
}

#[tokio::test]
async fn delivery_operations_map_pi_rejections_without_external_turn_identity() {
    for method in ["submit", "steer", "interrupt"] {
        for (code, expected) in [
            (
                -32601,
                DeliveryError::Unsupported("delivery diagnostic".into()),
            ),
            (-32006, DeliveryError::Offline("delivery diagnostic".into())),
            (
                if method == "submit" { -32010 } else { -32011 },
                DeliveryError::StateChanged("delivery diagnostic".into()),
            ),
            (
                -32007,
                DeliveryError::Uncertain("delivery diagnostic".into()),
            ),
        ] {
            let (left, right) = UnixStream::pair().unwrap();
            let (peer, _) = PiRpcPeer::new(left);
            let server = tokio::spawn(async move {
                let mut pi = BufReader::new(right);
                let mut line = String::new();
                pi.read_line(&mut line).await.unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], method);
                assert_eq!(request["params"].get("turn_id"), None);
                pi.get_mut()
                    .write_all(
                        format!(
                            "{}\n",
                            json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":code,"message":"delivery diagnostic"}})
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            });
            let payload = InputPayload::from("once");
            let operation = match method {
                "submit" => peer.start_turn(&payload, Some("message")),
                "steer" => peer.steer(&payload, Some("message")),
                "interrupt" => peer.interrupt(),
                _ => unreachable!(),
            };
            let error = operation.await.unwrap_err();
            assert_eq!(error, expected, "{method} mapped {code} incorrectly");
            server.await.unwrap();
            peer.close();
        }
    }
}

#[tokio::test]
async fn cancelling_read_only_history_does_not_break_control_or_accept_a_late_snapshot() {
    let (left, right) = UnixStream::pair().unwrap();
    let (peer, _) = PiRpcPeer::new(left);
    let mut runtime = BufReader::new(right);
    let pending = tokio::spawn({
        let peer = peer.clone();
        async move { peer.call("history.read", json!({})).await }
    });
    let mut line = String::new();
    runtime.read_line(&mut line).await.unwrap();
    let history: Value = serde_json::from_str(&line).unwrap();
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    runtime
        .get_mut()
        .write_all(
            format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":history["id"],"result":{}})
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let ping = tokio::spawn({
        let peer = peer.clone();
        async move { peer.call("ping", json!({})).await }
    });
    line.clear();
    runtime.read_line(&mut line).await.unwrap();
    let request: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(request["method"], "ping");
    runtime
        .get_mut()
        .write_all(
            format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":request["id"],"result":{"pong":true}})
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    assert_eq!(ping.await.unwrap().unwrap(), json!({"pong":true}));
    peer.close();
}
