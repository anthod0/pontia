use crate::{AgentControl, client_contract::ClientOperation};
use tokio::task::JoinHandle;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClientServicePhase {
    Transport,
    Observation,
}

/// Client-owned transports and observers hosted by the control-plane service.
pub trait ClientService: Send + Sync {
    fn uses_control_socket(&self) -> bool {
        false
    }

    fn accepts_unbound(&self, _method: &str) -> bool {
        false
    }

    fn connect(
        &self,
        _state: AgentControl,
        _process_id: Option<u32>,
        peer: std::sync::Arc<crate::ipc::rpc::RpcPeer>,
        _first: crate::ipc::rpc::RpcRequest,
        _requests: crate::ipc::rpc::Requests,
    ) -> ClientOperation<'_, ()> {
        Box::pin(async move {
            peer.close();
            Ok(())
        })
    }

    fn phase(&self) -> ClientServicePhase;

    /// Prepare and start background work, returning any completion the host must await.
    /// Detached work still subscribes to the application's shutdown signal.
    fn start<'a>(
        &'a self,
        state: AgentControl,
    ) -> ClientOperation<'a, Option<JoinHandle<pontia_core::Result<()>>>>;
}
