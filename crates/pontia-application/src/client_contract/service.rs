use crate::{AppState, client_contract::ClientOperation};
use tokio::task::JoinHandle;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClientServicePhase {
    Transport,
    Observation,
}

/// Client-owned transports and observers hosted by the control-plane service.
pub trait ClientService: Send + Sync {
    fn phase(&self) -> ClientServicePhase;

    /// Prepare and start background work, returning any completion the host must await.
    /// Detached work still subscribes to the application's shutdown signal.
    fn start<'a>(
        &'a self,
        state: AppState,
    ) -> ClientOperation<'a, Option<JoinHandle<pontia_core::Result<()>>>>;
}
