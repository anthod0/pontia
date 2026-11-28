#![allow(dead_code)]
use pontia_agent_control::ipc::rpc::{PROTOCOL_VERSION, RpcRequest};
use pontia_client_pi::rpc::PiRpcPeer;
use serde_json::json;
use std::sync::Arc;
use tokio::net::UnixStream;

pub fn clients() -> pontia_agent_control::clients::ClientRegistry {
    let mut clients = pontia_agent_control::clients::ClientRegistry::default();
    clients.register(pontia_client_pi::registration(None));
    clients
}

pub fn connection(
    state: &pontia_agent_control::AgentControl,
) -> (Arc<PiRpcPeer>, tokio::sync::mpsc::Receiver<RpcRequest>) {
    let (server, client) = UnixStream::pair().unwrap();
    tokio::spawn(pontia_client_pi::ipc::serve_connection(
        state.client_context(),
        server,
    ));
    PiRpcPeer::new(client)
}

pub async fn attach(
    state: &pontia_agent_control::AgentControl,
    session: &str,
    runtime: &str,
    native: &str,
) -> (Arc<PiRpcPeer>, tokio::sync::mpsc::Receiver<RpcRequest>) {
    let fingerprint = pontia_runtime::ProcessFingerprint::capture(std::process::id()).unwrap();
    sqlx::query(
        "UPDATE session_runtimes SET process_fingerprint=? WHERE runtime_id=? AND process_fingerprint IS NULL",
    )
    .bind(serde_json::to_string(&fingerprint).unwrap())
    .bind(runtime)
    .execute(&state.db())
    .await
    .unwrap();
    let (peer, requests) = connection(state);
    peer.call(
        "runtime.attach",
        json!({
            "version": PROTOCOL_VERSION,
            "session_id": session,
            "runtime_id": runtime,
            "client_session_key": native,
        }),
    )
    .await
    .unwrap();
    (peer, requests)
}

pub async fn connected(state: &pontia_agent_control::AgentControl, session: &str) -> bool {
    state
        .queries()
        .get_session(session)
        .await
        .unwrap()
        .unwrap()
        .control_connected
        == Some(true)
}

pub fn session(
    state: &pontia_agent_control::AgentControl,
) -> Arc<dyn pontia_agent_control::client_contract::ClientSession> {
    state.clients().get("pi").unwrap().session.clone().unwrap()
}

pub async fn target(
    state: &pontia_agent_control::AgentControl,
    session: &str,
) -> pontia_agent_control::runtime::ControlTarget {
    pontia_agent_control::runtime::ControlTarget::resolve(
        &state.db(),
        &state.clients(),
        session,
        None,
    )
    .await
    .unwrap()
}
