use pontia_agent_control::{
    ClientServiceContext,
    client_contract::{ClientOperation, ClientService, ClientServicePhase},
    ipc::rpc::RpcPeer,
};
use std::sync::Arc;
use tokio::task::JoinHandle;

pub(crate) struct PiService {
    pub(crate) connections: crate::connections::PiConnections,
}

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
        state: ClientServiceContext,
        process_id: Option<u32>,
        peer: Arc<RpcPeer>,
        first: pontia_agent_control::ipc::rpc::RpcRequest,
        requests: pontia_agent_control::ipc::rpc::Requests,
    ) -> ClientOperation<'_, ()> {
        let connections = self.connections.clone();
        Box::pin(async move {
            connections
                .ensure_started(state.store(), state.pontia_home().into())
                .await;
            crate::ipc::serve_peer(
                state,
                connections,
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
        state: ClientServiceContext,
    ) -> ClientOperation<'a, Option<JoinHandle<pontia_core::Result<()>>>> {
        let connections = self.connections.clone();
        Box::pin(async move {
            connections
                .start(state.store(), state.pontia_home().into())
                .await;
            let mut shutdown = state.shutdown().subscribe();
            Ok(Some(tokio::spawn(async move {
                let _ = shutdown.wait_for(|stop| *stop).await;
                connections.close().await;
                Ok(())
            })))
        })
    }
}
