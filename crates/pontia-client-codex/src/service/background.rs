use super::CodexObserver;
use pontia_application::{
    AppState,
    client_contract::{ClientOperation, ClientService, ClientServicePhase},
};
use tokio::task::JoinHandle;

pub(crate) struct CodexBackgroundService;

impl ClientService for CodexBackgroundService {
    fn phase(&self) -> ClientServicePhase {
        ClientServicePhase::Observation
    }

    fn start<'a>(
        &'a self,
        state: AppState,
    ) -> ClientOperation<'a, Option<JoinHandle<pontia_core::Result<()>>>> {
        Box::pin(async move {
            let observer = CodexObserver::new(
                state.event_ingest_service(),
                state.pontia_home().to_path_buf(),
            );
            observer.prepare().await?;
            tokio::spawn(observer.run(state.shutdown().subscribe()));
            Ok(None)
        })
    }
}
