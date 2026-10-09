use crate::ipc::PiIpcListener;
use pontia_application::{
    AppState,
    client_contract::{ClientOperation, ClientService, ClientServicePhase},
};
use tokio::task::JoinHandle;

pub(crate) struct PiService;

impl ClientService for PiService {
    fn phase(&self) -> ClientServicePhase {
        ClientServicePhase::Transport
    }

    fn start<'a>(
        &'a self,
        state: AppState,
    ) -> ClientOperation<'a, Option<JoinHandle<pontia_core::Result<()>>>> {
        Box::pin(async move {
            let listener = PiIpcListener::bind(state.pontia_home()).await?;
            let shutdown = state.shutdown().subscribe();
            Ok(Some(tokio::spawn(listener.run(state, shutdown))))
        })
    }
}
