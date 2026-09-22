use futures_util::{SinkExt, StreamExt};
use pontia_runtime::codex::protocol::Connection;
use serde_json::{Value, json};
use tokio::{
    net::UnixListener,
    time::{Duration, timeout},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn correlates_out_of_order_responses_and_leaves_questions_to_tui() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("server.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut wire = accept_async(stream).await.unwrap();
        let init: Value =
            serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(init["method"], "initialize");
        wire.send(Message::Text(
            json!({"id":init["id"],"result":{}}).to_string().into(),
        ))
        .await
        .unwrap();
        let initialized: Value =
            serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(initialized["method"], "initialized");
        let first: Value =
            serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        let second: Value =
            serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        wire.send(Message::Text(json!({"id":70,"method":"item/tool/requestUserInput","params":{"threadId":"thread","isBlocking":true}}).to_string().into())).await.unwrap();
        for request in [second, first] {
            wire.send(Message::Text(
                json!({"id":request["id"],"result":{"method":request["method"]}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        }
        // There is no automatic answer, cancellation or denial from the observer.
        assert!(
            timeout(Duration::from_millis(50), wire.next())
                .await
                .is_err()
        );
    });
    let connection = Connection::connect(&socket).await.unwrap();
    let mut events = connection.events.subscribe();
    let (first, second) = tokio::join!(
        connection.call("turn/start", json!({})),
        connection.call("thread/read", json!({}))
    );
    assert_eq!(first.unwrap()["method"], "turn/start");
    assert_eq!(second.unwrap()["method"], "thread/read");
    assert_eq!(
        events.recv().await.unwrap()["method"],
        "item/tool/requestUserInput"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn disconnect_after_sending_input_returns_uncertainty_without_retry() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("server.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut wire = accept_async(stream).await.unwrap();
        let init: Value =
            serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        wire.send(Message::Text(
            json!({"id":init["id"],"result":{}}).to_string().into(),
        ))
        .await
        .unwrap();
        wire.next().await.unwrap().unwrap();
        let input: Value =
            serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(input["method"], "turn/start");
        drop(wire);
        assert!(
            timeout(Duration::from_millis(80), listener.accept())
                .await
                .is_err()
        );
    });
    let connection = Connection::connect(&socket).await.unwrap();
    let error = connection
        .call("turn/start", json!({"input":"hello"}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("uncertain"));
    server.await.unwrap();
    assert!(!connection.is_connected());
}
