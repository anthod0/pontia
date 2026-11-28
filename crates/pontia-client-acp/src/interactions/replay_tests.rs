use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use pontia_agent_control::{
    AgentControl,
    clients::ClientRegistry,
    interactions::InteractionError,
    ipc::rpc::{MAX_FRAME_BYTES, RpcPeer},
};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

async fn frame(remote: &RpcPeer, key: &str, content: &Value) -> Value {
    remote
        .request(
            "relay.frame",
            json!({"frame_id":key, "last":true,
        "chunk":STANDARD.encode(content.to_string())}),
            MAX_FRAME_BYTES,
            false,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn permission_replay_keeps_one_responder_and_conflict_cancels_it_once() {
    let root = tempfile::tempdir().unwrap();
    let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&db).await.unwrap();
    sqlx::raw_sql("INSERT INTO sessions(session_id,client_type,state,current_turn_id) VALUES ('s','acp','busy','t');
        INSERT INTO turns(turn_id,session_id,state) VALUES ('t','s','running');
        INSERT INTO session_runtimes(runtime_id,session_id,role,state,created_at) VALUES ('r','s','acp','running','2026-01-01T00:00:00Z');")
        .execute(&db).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(crate::registration());
    let app = AgentControl::builder(db.clone(), root.path().into())
        .clients(clients)
        .build();
    let (agent, controller) = UnixStream::pair().unwrap();
    let (remote, mut outgoing) = RpcPeer::new(agent);
    let (rpc, incoming) = RpcPeer::new(controller);
    let relay = Arc::new(crate::transport::Relay {
        rpc,
        process: pontia_runtime::ProcessFingerprint::capture(std::process::id()).unwrap(),
    });
    let peer = Peer::connect(
        &app,
        crate::peer::PeerIdentity {
            session: "s",
            runtime: "r",
        },
        root.path(),
        relay,
        incoming,
        Value::Null,
        crate::peer::PeerOwner {
            policy: Default::default(),
            peers: Default::default(),
            reconnect: mpsc::unbounded_channel().0,
            agent: crate::capabilities::AgentBounds::of("test", None),
        },
    )
    .await
    .unwrap();
    {
        let mut state = peer.state.lock().await;
        state.native = Some(SessionId::new("native"));
        state.active = Some(crate::peer::Prompt {
            request_id: json!(99),
            turn: "t".into(),
            started: true,
            sequence: 0,
            raw_results: Default::default(),
            thought: None,
            cancelled: false,
            prompt_index: None,
            history_index: 0,
            history_end: 0,
        });
    }
    let (responses, mut wire) = mpsc::unbounded_channel();
    let endpoint = remote.clone();
    let responder = tokio::spawn(async move {
        while let Some(req) = outgoing.recv().await {
            assert_eq!(req.method, "relay.write");
            let bytes = STANDARD
                .decode(req.params["chunk"].as_str().unwrap())
                .unwrap();
            responses
                .send(serde_json::from_slice::<Value>(&bytes).unwrap())
                .unwrap();
            endpoint.reply(req.id, json!({})).await.unwrap();
        }
    });
    let mut permission = json!({"method":"session/request_permission","jsonrpc":"2.0","id":42,
        "params":{"sessionId":"native","toolCall":{"toolCallId":"tool","rawInput":{"path":"original"}},
            "options":[{"optionId":"allow","name":"Once","kind":"allow_once"}]}});
    assert!(
        frame(&remote, "frame-1", &permission)
            .await
            .get("result")
            .is_some()
    );
    let replay = frame(&remote, "frame-1", &permission).await;
    let first = replay["result"]["interaction_id"].as_str().unwrap();
    resolve(&peer, first, "allow").await.unwrap();
    let response = wire.recv().await.unwrap();
    assert_eq!(response["id"], 42);
    assert_eq!(response["result"]["outcome"]["optionId"], "allow");
    assert_eq!(
        frame(&remote, "frame-1", &permission).await["result"]["interaction_id"],
        first
    );

    permission["id"] = json!(43);
    frame(&remote, "frame-2", &permission).await;
    let replay = frame(&remote, "frame-2", &permission).await;
    let second = replay["result"]["interaction_id"].as_str().unwrap();
    permission["params"]["toolCall"]["rawInput"]["path"] = json!("changed");
    for _ in 0..2 {
        assert_eq!(
            frame(&remote, "frame-2", &permission).await["error"]["code"],
            -32009
        );
    }
    let cancelled = wire.recv().await.unwrap();
    assert_eq!(cancelled["id"], 43);
    assert_eq!(cancelled["result"]["outcome"]["outcome"], "cancelled");
    assert!(matches!(
        app.interactions().respond(&app, "s", second, "allow").await,
        Err(InteractionError::Expired)
    ));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), wire.recv())
            .await
            .is_err()
    );
    let keys: Vec<String> =
        sqlx::query_scalar("SELECT report_key FROM interactions ORDER BY report_key")
            .fetch_all(&db)
            .await
            .unwrap();
    assert_eq!(keys, ["acp:frame:frame-1", "acp:frame:frame-2"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM events WHERE event_type='interaction.blocked'"
        )
        .fetch_one(&db)
        .await
        .unwrap(),
        2
    );
    peer.stop().await;
    remote.close();
    responder.await.unwrap();
}
