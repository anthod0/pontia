use super::CodexObserver;
use pontia_agent_control::{
    AgentControl,
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
        state: AgentControl,
    ) -> ClientOperation<'a, Option<JoinHandle<pontia_core::Result<()>>>> {
        Box::pin(async move {
            let observer = CodexObserver::new(&state, state.pontia_home().to_path_buf());
            observer.prepare().await?;
            let proxy = crate::proxy::Proxy::bind(state.pontia_home()).await?;
            tokio::spawn(observer.run(state.shutdown().subscribe()));
            let (observations, incoming) = tokio::sync::mpsc::channel(128);
            Ok(Some(tokio::spawn(async move {
                tokio::select! {
                    result = proxy.run(state.shutdown().subscribe(), || {
                        crate::runtime::daemon::Endpoint::resolve().map(|endpoint| endpoint.socket)
                    }, observations, state.clone()) => result,
                    _ = super::proxy_observer::run(state.clone(), incoming) => Ok(()),
                }
            })))
        })
    }
}
