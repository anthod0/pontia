use pontia_application::{AppState, LiveOutputItem, pi_ipc::serve_connection};
use pontia_runtime::pi_control::{PROTOCOL_VERSION, PiRpcPeer};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::net::UnixStream;

struct Session {
    state: AppState,
    peer: Arc<PiRpcPeer>,
    session: String,
    runtime: String,
    turn: String,
    _root: tempfile::TempDir,
}

impl Session {
    async fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("pi-live-")
            .tempdir()
            .unwrap();
        let db = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
            .await
            .unwrap();
        run_migrations(&db).await.unwrap();
        let state = AppState::builder(db, root.path().into()).build();
        let peer = connect(&state);
        let registered = peer.call("runtime.register", json!({
            "version": PROTOCOL_VERSION,
            "binding": {
                "client_type": "pi", "client_session_key": "native", "client_cwd": root.path(),
                "tmux": {"socket_path": root.path().join("tmux.sock"), "pane_id": "%1"}
            }
        })).await.unwrap();
        let session = registered["session"]["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let runtime = registered["runtime"]["runtime_instance_id"]
            .as_str()
            .unwrap()
            .to_owned();
        peer.call(
            "event.report",
            json!({
                "runtime_instance_id": runtime,
                "event": {"session_id": session, "type": "session.ready", "data": {
                    "runtime_instance_id": runtime, "client_session_key": "native"
                }}
            }),
        )
        .await
        .unwrap();
        let started = peer
            .call(
                "event.report",
                json!({
                    "runtime_instance_id": runtime,
                    "event": {"session_id": session, "type": "turn.started", "data": {
                        "runtime_instance_id": runtime, "input_summary": "test"
                    }}
                }),
            )
            .await
            .unwrap();
        let turn = started["turn_id"].as_str().unwrap().to_owned();
        Self {
            state,
            peer,
            session,
            runtime,
            turn,
            _root: root,
        }
    }

    fn request(&self, operation: Value) -> Value {
        let mut params = json!({
            "session_id": self.session, "runtime_instance_id": self.runtime,
            "turn_id": self.turn, "stream_id": "stream_live"
        });
        params
            .as_object_mut()
            .unwrap()
            .extend(operation.as_object().unwrap().clone());
        params
    }

    async fn publish(&self, operation: Value) -> Value {
        self.peer
            .call("liveOutput.publish", self.request(operation))
            .await
            .unwrap()
    }

    async fn event_count(&self) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&self.state.db())
            .await
            .unwrap()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.peer.close();
    }
}

fn connect(state: &AppState) -> Arc<PiRpcPeer> {
    let (server, client) = UnixStream::pair().unwrap();
    tokio::spawn(serve_connection(state.clone(), server));
    PiRpcPeer::new(client).0
}

fn snapshot(sequence: u64, text: &str) -> Value {
    json!({"type": "snapshot", "sequence": sequence,
        "items": [{"kind": "assistant_text", "item_id": "text_1", "text": text}]})
}

#[tokio::test]
async fn orders_deduplicates_and_resynchronizes_live_output_without_persisting_it() {
    let session = Session::new().await;
    let event_count = session.event_count().await;
    assert_eq!(
        session.publish(snapshot(1, "hello")).await["accepted_sequence"],
        1
    );
    let append = json!({"type": "append", "first_sequence": 2, "updates": [
        {"type": "assistant_text_delta", "item_id": "text_1", "delta": " world"},
        {"type": "tool_call", "item_id": "tool_1", "call_id": "call_1", "tool_name": "read",
            "arguments": {"path": "src/main.rs"}, "managed_tool_use": {
                "tool_name": "read", "input": {"type": "read", "path": "src/main.rs"}
            }},
        {"type": "assistant_text_delta", "item_id": "text_2", "delta": "done"}
    ]});
    assert_eq!(
        session.publish(append.clone()).await,
        json!({
            "accepted": true, "duplicate": false, "resync_required": false, "accepted_sequence": 4
        })
    );
    assert_eq!(session.publish(append).await["duplicate"], true);
    let current = session
        .state
        .live_output()
        .snapshot(&session.session, &session.turn)
        .unwrap();
    assert_eq!(current.sequence, 4);
    assert_eq!(
        serde_json::to_value(current.items).unwrap(),
        json!([
            {"kind": "assistant_text", "item_id": "text_1", "text": "hello world"},
            {"kind": "tool_call", "item_id": "tool_1", "call_id": "call_1", "tool_name": "read",
                "arguments": {"path": "src/main.rs"}, "managed_tool_use": {
                    "tool_name": "read", "input": {"type": "read", "path": "src/main.rs"}
                }},
            {"kind": "assistant_text", "item_id": "text_2", "text": "done"}
        ])
    );
    assert_eq!(
        session
            .publish(json!({"type": "append", "first_sequence": 6, "updates": [
                {"type": "assistant_text_delta", "item_id": "text_2", "delta": "!"}
            ]}))
            .await,
        json!({
            "accepted": false, "duplicate": false, "resync_required": true, "accepted_sequence": 4
        })
    );
    assert_eq!(
        session.publish(snapshot(6, "resynchronized")).await["accepted_sequence"],
        6
    );
    assert_eq!(
        session
            .state
            .live_output()
            .snapshot(&session.session, &session.turn)
            .unwrap()
            .items,
        vec![LiveOutputItem::AssistantText {
            item_id: "text_1".into(),
            text: "resynchronized".into()
        }]
    );
    let close = json!({"type": "stream_closed", "sequence": 7});
    assert_eq!(session.publish(close.clone()).await["accepted_sequence"], 7);
    assert_eq!(session.publish(close).await["duplicate"], true);
    assert!(
        session
            .state
            .live_output()
            .snapshot(&session.session, &session.turn)
            .is_none()
    );
    assert_eq!(session.event_count().await, event_count);
}

#[tokio::test]
async fn rejects_invalid_tool_payloads_and_unregistered_or_mismatched_identities() {
    let session = Session::new().await;
    let unregistered = connect(&session.state);
    let error = unregistered
        .call("liveOutput.publish", session.request(snapshot(1, "hello")))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("-32009"), "{error}");
    unregistered.close();
    for overrides in [
        json!({"session_id": "another_session"}),
        json!({"runtime_instance_id": "stale_runtime"}),
        json!({"turn_id": "missing_turn"}),
        json!({"sequence": "invalid"}),
        json!({"unexpected": true}),
        json!({"items": [{"kind": "tool_call", "item_id": "tool_1", "call_id": "call_1",
        "tool_name": "read", "arguments": {"path": "src/main.rs"}, "managed_tool_use": {
            "tool_name": "read", "input": {"type": "bash", "command": "cat src/main.rs"}
        }}]}),
    ] {
        let mut params = session.request(snapshot(1, "hello"));
        params
            .as_object_mut()
            .unwrap()
            .extend(overrides.as_object().unwrap().clone());
        assert!(
            session
                .peer
                .call("liveOutput.publish", params)
                .await
                .is_err()
        );
        assert!(
            session
                .state
                .live_output()
                .snapshot(&session.session, &session.turn)
                .is_none()
        );
    }
    assert_eq!(
        session.publish(snapshot(1, "valid")).await["accepted"],
        true
    );
}

#[tokio::test]
async fn checks_current_runtime_even_when_connection_identity_matches() {
    let session = Session::new().await;
    sqlx::query("UPDATE runtime_bindings SET runtime_instance_id='replacement' WHERE session_id=?")
        .bind(&session.session)
        .execute(&session.state.db())
        .await
        .unwrap();
    let error = session
        .peer
        .call("liveOutput.publish", session.request(snapshot(1, "stale")))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("-32009"), "{error}");
    assert!(
        session
            .state
            .live_output()
            .snapshot(&session.session, &session.turn)
            .is_none()
    );
}

#[tokio::test]
async fn terminal_facts_clear_live_output_and_reject_further_updates() {
    let session = Session::new().await;
    session.publish(snapshot(1, "hello")).await;
    let result = session
        .peer
        .call(
            "event.report",
            json!({
                "runtime_instance_id": session.runtime,
                "event": {"session_id": session.session, "turn_id": session.turn,
                    "type": "turn.completed", "data": {"terminal_leaf_id": null}}
            }),
        )
        .await
        .unwrap();
    assert_eq!(result["accepted"], true);
    assert!(
        session
            .state
            .live_output()
            .snapshot(&session.session, &session.turn)
            .is_none()
    );
    assert!(
        session
            .peer
            .call("liveOutput.publish", session.request(snapshot(2, "late")))
            .await
            .is_err()
    );
}
