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
    fn record_protocol_rejection<'a>(
        &'a self,
        state: &'a ClientServiceContext,
        process_id: Option<u32>,
        request: &'a pontia_agent_control::ipc::rpc::RpcRequest,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let _guard = self.connections.lock_identity().await;
            pontia_agent_control::ipc::protocol::record_rejection(
                state,
                request,
                process_id,
                Some(crate::SPEC.client_type),
                async |session, runtime| self.connections.connected_to(session, runtime).await,
            )
            .await
        })
    }
    fn connect(
        &self,
        state: ClientServiceContext,
        process_id: Option<u32>,
        peer: Arc<RpcPeer>,
        first: pontia_agent_control::ipc::rpc::RpcRequest,
        requests: pontia_agent_control::ipc::rpc::Requests,
    ) -> ClientOperation<'_, ()> {
        Box::pin(async move {
            self.connections
                .ensure_started(state.store(), state.pontia_home().into())
                .await;
            crate::ipc::serve_peer(state, self, process_id, peer, Some(first), requests).await;
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
