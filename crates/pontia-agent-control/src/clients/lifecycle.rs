use super::ClientAdapter;
use crate::client_contract::{ClientExitMode, ClientExitOutcome, TerminateBehavior};
use crate::{control::ControlResult, runtime::ControlTarget};
use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult};
use std::path::Path;

impl ClientAdapter {
    pub async fn start(
        &self,
        root: &Path,
        request: RuntimeStartRequest,
        configuration: Option<serde_json::Value>,
    ) -> Result<Option<RuntimeStartResult>> {
        let client = self.session_client().ok_or_else(|| {
            Error::CapabilityUnavailable("Client Session control is unavailable".into())
        })?;
        match configuration {
            Some(configuration) => {
                client
                    .provision_configured(self.events.clone(), root, request, configuration)
                    .await
            }
            None => client.provision(self.events.clone(), root, request).await,
        }
    }

    pub async fn exit(
        &self,
        target: &ControlTarget,
        mode: ClientExitMode,
    ) -> ControlResult<ClientExitOutcome> {
        let result = async {
            target.validate(&self.pool).await?;
            if let Some(client) = self.session_client() {
                return client.exit(self.events.clone(), target, mode).await;
            }
            match self.spec.adapter.terminate {
                TerminateBehavior::Connected => {
                    return Err(Error::CapabilityUnavailable(format!(
                        "Client {} has no Session control implementation",
                        self.spec.client_type
                    )));
                }
                TerminateBehavior::RuntimeManager => {
                    // The input reservation serializes backend-managed, in-process Turns.
                    if mode == ClientExitMode::IfIdle
                        && !matches!(
                            self.spec.adapter.runtime,
                            crate::client_contract::RuntimeBehavior::InProcess
                        )
                    {
                        return Err(Error::CapabilityUnavailable(
                            "Client cannot exit conditionally while idle".into(),
                        ));
                    }
                    if let Some(handle) = &target.runtime_id {
                        GenericRuntimeManager.terminate_session(handle)?;
                    }
                }
            }
            Ok(ClientExitOutcome::Requested)
        }
        .await;
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
        let binding = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_session(&target.session_id)
            .await?;
        let pane = target.tmux_pane(&self.pool.clone()).await.ok();
        target.validate(&self.pool.clone()).await?;
        if let Some(client) = self.session_client() {
            return client
                .resume_with_runtime(
                    self.events.clone(),
                    target,
                    super::ClientLaunchRequest {
                        root,
                        runtime: request,
                        restart_count: count,
                        reuse_pane: pane
                            .as_ref()
                            .map(|(socket, pane)| (socket.as_str(), pane.as_str())),
                        native_session_key: binding
                            .as_ref()
                            .map(|binding| binding.client_session_key.as_str()),
                        native_session_file: binding
                            .as_ref()
                            .and_then(|binding| binding.client_session_file.as_deref())
                            .map(Path::new),
                    },
                )
                .await;
        }
        Err(Error::CapabilityUnavailable(
            "Client Session control is unavailable".into(),
        ))
    }

    pub async fn validate_resume(&self, target: &ControlTarget) -> Result<()> {
        target.validate(&self.pool).await?;
        if let Some(client) = self.session_client() {
            return client.validate_resume(self.pool.clone(), target).await;
        }
        Err(Error::CapabilityUnavailable(
            "Client Session control is unavailable".into(),
        ))
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
        let binding = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_session(&target.session_id)
            .await?;
        match self.spec.adapter.terminate {
            TerminateBehavior::Connected => {
                if let Some(runtime_id) = &target.runtime_id
                    && let Some(record) = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(self.pool.clone()).get(runtime_id).await?
                {
                    crate::runtime::terminate_runtime(record.tmux_socket_path.as_deref(), record.tmux_pane_id.as_deref(), record.process_fingerprint.as_deref())?;
                }
            }
            TerminateBehavior::RuntimeManager => {
                self.exit(target, ClientExitMode::Unconditional).await.into_result()?;
            }
        }
        if let Some(client) = self.session_client() {
            return client
                .resume_with_runtime(
                    self.events.clone(),
                    target,
                    super::ClientLaunchRequest {
                        root,
                        runtime: request,
                        restart_count: count,
                        reuse_pane: None,
                        native_session_key: binding
                            .as_ref()
                            .map(|binding| binding.client_session_key.as_str()),
                        native_session_file: binding
                            .as_ref()
                            .and_then(|binding| binding.client_session_file.as_deref())
                            .map(Path::new),
                    },
                )
                .await?
                .ok_or_else(|| {
                    Error::CapabilityUnavailable("Client restart did not start a runtime".into())
                });
        }
        Err(Error::CapabilityUnavailable(
            "Client Session control is unavailable".into(),
        ))
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
    if let Some(value) = runtime.metadata.get("process_fingerprint") {
        if let Ok(fingerprint) = serde_json::from_value(value.clone()) {
            let _ = GenericRuntimeManager.terminate_process(&fingerprint);
        }
    } else {
        let _ = GenericRuntimeManager.terminate_session(&runtime.runtime_handle);
    }
}
