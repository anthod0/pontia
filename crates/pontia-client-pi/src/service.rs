use pontia_agent_control::{
    AgentControl,
    client_contract::{ClientOperation, ClientService, ClientServicePhase},
    ipc::rpc::RpcPeer,
};
use std::sync::Arc;
use tokio::task::JoinHandle;

pub(crate) struct PiService;

impl ClientService for PiService {
    fn uses_control_socket(&self) -> bool {
        true
    }

    fn phase(&self) -> ClientServicePhase {
        ClientServicePhase::Transport
    }
    fn accepts_unbound(&self, method: &str) -> bool {
        matches!(
            method,
            "session.get" | "workspaces.list" | "session.context"
        )
    }
    fn connect(
        &self,
        state: AgentControl,
        process_id: Option<u32>,
        peer: Arc<RpcPeer>,
        first: pontia_agent_control::ipc::rpc::RpcRequest,
        requests: pontia_agent_control::ipc::rpc::Requests,
    ) -> ClientOperation<'_, ()> {
        Box::pin(async move {
            crate::ipc::serve_peer(
                state,
                process_id,
                crate::rpc::PiRpcPeer::from_rpc(peer),
                Some(first),
                requests,
            )
            .await;
            Ok(())
        })
    }
    fn start<'a>(
        &'a self,
        _state: AgentControl,
    ) -> ClientOperation<'a, Option<JoinHandle<pontia_core::Result<()>>>> {
        Box::pin(async { Ok(None) })
    }
}
