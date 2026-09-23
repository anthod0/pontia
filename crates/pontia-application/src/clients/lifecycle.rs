use super::ClientAdapter;
use crate::client_contract::TerminateBehavior;
use crate::{control::ControlResult, runtime::ControlTarget};
use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult};
use std::path::Path;

impl ClientAdapter {
    pub(crate) async fn open_interface(&self, session_id: &str) -> Result<()> {
        let client = self.session_client().ok_or_else(|| {
            Error::CapabilityUnavailable("Client interface is unavailable".into())
        })?;
        client.open_interface(self.events.clone(), session_id).await
    }

    fn start_in_process(
        &self,
        root: &Path,
        request: RuntimeStartRequest,
        count: i64,
    ) -> Result<RuntimeStartResult> {
        let clients = &self.registry;
        let client = clients
            .get(self.spec.client_type)
            .and_then(|entry| entry.in_process.as_ref())
            .ok_or_else(|| Error::CapabilityUnavailable("Client launcher is unavailable".into()))?;
        GenericRuntimeManager.start_in_process(root, request, client.capabilities(), count)
    }

    pub async fn start(
        &self,
        root: &Path,
        request: RuntimeStartRequest,
    ) -> Result<Option<RuntimeStartResult>> {
        if let Some(client) = self.session_client() {
            client.provision(self.events.clone(), root, request).await?;
            return Ok(None);
        }
        if let Some(launcher) = self
            .registry
            .get(self.spec.client_type)
            .and_then(|entry| entry.launcher.as_ref())
        {
            return launcher
                .launch(super::ClientLaunchRequest {
                    root,
                    runtime: request,
                    restart_count: 0,
                    reuse_pane: None,
                    native_session_key: None,
                })
                .map(Some);
        }
        self.start_in_process(root, request, 0).map(Some)
    }

    pub async fn exit(&self, target: &ControlTarget) -> ControlResult<()> {
        let result = async {
            target.validate(&self.pool.clone()).await?;
            if let Some(client) = self.session_client() {
                return client.exit(self.events.clone(), target).await;
            }
            match self.spec.adapter.terminate {
                TerminateBehavior::Connected => self.control
                    .shutdown(&target.session_id, target.instance()?).await,
                TerminateBehavior::RuntimeManager => {
                    if let Some(handle) = pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository::new(self.pool.clone()).runtime_handle(&target.session_id).await? {
                        GenericRuntimeManager.terminate_session(&handle)?;
                    }
                    Ok(())
                }
            }
        }.await;
        ControlResult::from_result(result)
    }

    pub async fn resume(
        &self,
        target: &ControlTarget,
        root: &Path,
        request: RuntimeStartRequest,
        count: i64,
    ) -> Result<Option<RuntimeStartResult>> {
        target.validate(&self.pool.clone()).await?;
        if let Some(client) = self.session_client() {
            client.resume(self.events.clone(), target).await?;
            return Ok(None);
        }
        let binding = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_session(&target.session_id)
            .await?;
        let pane = target.tmux_pane(&self.pool.clone()).await.ok();
        target.validate(&self.pool.clone()).await?;
        if let Some(launcher) = self
            .registry
            .get(self.spec.client_type)
            .and_then(|entry| entry.launcher.as_ref())
        {
            return launcher
                .launch(super::ClientLaunchRequest {
                    root,
                    runtime: request,
                    restart_count: count,
                    reuse_pane: pane
                        .as_ref()
                        .map(|(socket, pane)| (socket.as_str(), pane.as_str())),
                    native_session_key: binding
                        .as_ref()
                        .map(|binding| binding.client_session_key.as_str()),
                })
                .map(Some);
        }
        self.start_in_process(root, request, count).map(Some)
    }

    pub async fn restart(
        &self,
        target: &ControlTarget,
        root: &Path,
        request: RuntimeStartRequest,
        count: i64,
    ) -> Result<RuntimeStartResult> {
        if !self.supports_restart() {
            return Err(Error::CapabilityUnavailable(
                "A Session cannot restart a shared runtime".into(),
            ));
        }
        target.validate(&self.pool.clone()).await?;
        match self.spec.adapter.terminate {
            TerminateBehavior::Connected => {
                let (socket, pane) = target.tmux_pane(&self.pool.clone()).await?;
                GenericRuntimeManager.kill_tmux_pane(&socket, &pane)?;
            }
            TerminateBehavior::RuntimeManager => {
                self.exit(target).await.into_result()?;
            }
        }
        if let Some(launcher) = self
            .registry
            .get(self.spec.client_type)
            .and_then(|entry| entry.launcher.as_ref())
        {
            return launcher.launch(super::ClientLaunchRequest {
                root,
                runtime: request,
                restart_count: count,
                reuse_pane: None,
                native_session_key: None,
            });
        }
        self.start_in_process(root, request, count)
    }

    pub async fn ensure_exit_available(&self, target: &ControlTarget) -> Result<()> {
        target.validate(&self.pool.clone()).await?;
        match self.spec.adapter.terminate {
            TerminateBehavior::Connected => {
                if !self.input_available(&target.session_id).await? {
                    return Err(Error::CapabilityUnavailable(
                        "client control channel is unavailable".into(),
                    ));
                }
            }
            TerminateBehavior::RuntimeManager => {}
        }
        Ok(())
    }
}

pub(crate) fn discard_unbound_runtime(runtime: &RuntimeStartResult) {
    let _ = GenericRuntimeManager.terminate_session(&runtime.runtime_handle);
}
