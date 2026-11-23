pub mod identity;
mod listener;
pub mod protocol;
pub mod rpc;
use crate::AgentControl;
pub use listener::{ControlListener, control_socket_path};
use rpc::{Requests, RpcPeer};
use std::{sync::Arc, time::Duration};
use tokio::net::UnixStream;

pub fn connection(stream: UnixStream) -> (Option<u32>, Arc<RpcPeer>, Requests) {
    let process_id = stream
        .peer_cred()
        .ok()
        .and_then(|credentials| credentials.pid())
        .and_then(|pid| u32::try_from(pid).ok());
    let (peer, requests) = RpcPeer::new(stream);
    (process_id, peer, requests)
}

struct ClosePeer(Arc<RpcPeer>);
impl Drop for ClosePeer {
    fn drop(&mut self) {
        self.0.close();
    }
}

pub async fn serve_connection(state: AgentControl, stream: UnixStream) {
    let (process_id, peer, mut requests) = connection(stream);
    let _close = ClosePeer(peer.clone());
    let Ok(Some(first)) = tokio::time::timeout(Duration::from_secs(10), requests.recv()).await
    else {
        peer.close();
        return;
    };
    if let Some(message) = protocol::rejection(&state, process_id, &first).await {
        let _ = peer
            .reply_error(first.id, protocol::UNSUPPORTED_VERSION, &message)
            .await;
        return;
    }
    let clients = state.clients();
    let client = match first.method.as_str() {
        "runtime.register" => first.params["binding"]["client_type"]
            .as_str()
            .map(str::to_owned),
        "runtime.attach" => {
            let session = first.params["session_id"].as_str().unwrap_or_default();
            match sqlx::query_scalar::<_, String>(
                "SELECT client_type FROM sessions WHERE session_id=?",
            )
            .bind(session)
            .fetch_optional(&state.db())
            .await
            {
                Ok(client) => client,
                Err(error) => {
                    tracing::warn!(%error, "Runtime attachment lookup failed");
                    let _ = peer
                        .reply_error(first.id, -32603, "Runtime attachment lookup failed")
                        .await;
                    return;
                }
            }
        }
        method => clients
            .registrations()
            .find(|registration| {
                registration
                    .service
                    .as_ref()
                    .is_some_and(|service| service.accepts_unbound(method))
            })
            .map(|registration| registration.spec.client_type.to_owned()),
    };
    let service = client
        .as_deref()
        .and_then(|client| clients.get(client))
        .and_then(|registration| registration.service.clone());
    match service {
        Some(service) => {
            if let Err(error) = service
                .connect(state, process_id, peer.clone(), first, requests)
                .await
            {
                tracing::warn!(%error, "Client control connection ended");
            }
        }
        None => {
            let _ = peer
                .reply_error(first.id, -32601, "Unknown client control connection")
                .await;
        }
    }
    peer.close();
}
