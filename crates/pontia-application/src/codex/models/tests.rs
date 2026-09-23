use super::{list_models, update_model};
use futures_util::{SinkExt, StreamExt};
use pontia_runtime::codex::protocol::Connection;
use serde_json::{Value, json};
use tokio::net::UnixListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::test]
async fn model_control_uses_native_catalog_and_settings_protocol() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("models.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut wire = accept_async(stream).await.unwrap();
        let init: Value =
            serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(init["params"]["capabilities"]["experimentalApi"], true);
        wire.send(Message::Text(
            json!({"id":init["id"],"result":{}}).to_string().into(),
        ))
        .await
        .unwrap();
        wire.next().await.unwrap().unwrap();
        for (cursor, data, next) in [
            (
                Value::Null,
                json!([{"id":"catalog-a","model":"model-a","displayName":"Model A","description":"First"}]),
                json!("page-2"),
            ),
            (
                json!("page-2"),
                json!([{"model":"hidden","displayName":"Hidden","hidden":true},{"id":"catalog-b","model":"model-b","displayName":"Model B"}]),
                Value::Null,
            ),
        ] {
            let request: Value =
                serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(request["method"], "model/list");
            assert_eq!(request["params"]["cursor"], cursor);
            assert_eq!(request["params"]["includeHidden"], false);
            wire.send(Message::Text(
                json!({"id":request["id"],"result":{"data":data,"nextCursor":next}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        }
        let request: Value =
            serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(request["method"], "thread/settings/update");
        assert_eq!(
            request["params"],
            json!({"threadId":"thread-a","model":"model-b"})
        );
        wire.send(Message::Text(
            json!({"id":request["id"],"result":{}}).to_string().into(),
        ))
        .await
        .unwrap();
    });
    let connection = Connection::connect(&socket).await.unwrap();
    let models = list_models(&connection).await.unwrap();
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["model-a", "model-b"]
    );
    assert_eq!(models[0].name, "Model A");
    assert_eq!(models[0].description, "First");
    update_model(&connection, "thread-a", "model-b")
        .await
        .unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn repeated_model_cursor_fails_instead_of_looping() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("models.sock");
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
        for _ in 0..2 {
            let request: Value =
                serde_json::from_str(wire.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            wire.send(Message::Text(
                json!({"id":request["id"],"result":{"data":[],"nextCursor":"repeat"}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        }
    });
    let connection = Connection::connect(&socket).await.unwrap();
    assert!(list_models(&connection).await.is_err());
    server.await.unwrap();
}
