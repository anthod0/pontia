use super::ClientAdapter;
use crate::client_contract::{ClientExitMode, ClientExitOutcome, TerminateBehavior};
use crate::{control::ControlResult, runtime::ControlTarget};
use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult};
use pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository;
use std::path::Path;

impl ClientAdapter {
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
        configuration: Option<serde_json::Value>,
    ) -> Result<Option<RuntimeStartResult>> {
        if let Some(client) = self.session_client() {
            let runtime = if let Some(configuration) = configuration {
                client
                    .provision_configured(self.events.clone(), root, request, configuration)
                    .await?
            } else {
                client.provision(self.events.clone(), root, request).await?
            };
            return Ok(runtime);
        }
        if let Some(launcher) = self
            .registry
            .get(self.spec.client_type)
            .and_then(|entry| entry.launcher.as_ref())
        {
            let request = super::ClientLaunchRequest {
                root,
                runtime: request,
                restart_count: 0,
                reuse_pane: None,
                native_session_key: None,
                native_session_file: None,
            };
            return match configuration {
                Some(configuration) => launcher.launch_configured(request, &configuration),
                None => launcher.launch(request),
            }
            .map(Some);
        }
        self.start_in_process(root, request, 0).map(Some)
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
                    let instance = target.instance()?;
                    if mode == ClientExitMode::IfIdle {
                        if !self
                            .control
                            .shutdown_if_idle(&target.session_id, instance)
                            .await?
                        {
                            return Ok(ClientExitOutcome::Busy);
                        }
                    } else if let Err(error) =
                        self.control.shutdown(&target.session_id, instance).await
                    {
                        if !matches!(error, Error::CapabilityUnavailable(_)) {
                            return Err(error);
                        }
                        let _guard = self.control.lock_identity().await;
                        target.validate(&self.pool).await?;
                        if self
                            .control
                            .connected_to(&target.session_id, instance)
                            .await
                        {
                            return Err(error);
                        }
                        let Some(fingerprint) = self.exit_process_fingerprint(target).await? else {
                            return Err(error);
                        };
                        GenericRuntimeManager.terminate_process(&fingerprint)?;
                    }
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
                    native_session_file: binding
                        .as_ref()
                        .and_then(|binding| binding.client_session_file.as_deref())
                        .map(Path::new),
                })
                .map(Some);
        }
        self.start_in_process(root, request, count).map(Some)
    }

    pub async fn validate_resume(&self, target: &ControlTarget) -> Result<()> {
        target.validate(&self.pool).await?;
        let binding = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_session(&target.session_id)
            .await?;
        if let Some(client) = self.session_client() {
            return client.validate_resume(self.pool.clone(), target).await;
        }
        self.validate_bound_session(binding.as_ref())
    }

    fn validate_bound_session(&self, binding: Option<&crate::AgentBinding>) -> Result<()> {
        if let Some(binding) = binding
            && let Some(launcher) = self
                .registry
                .get(self.spec.client_type)
                .and_then(|entry| entry.launcher.as_ref())
        {
            launcher.validate_resume(
                &binding.client_session_key,
                binding.client_session_file.as_deref().map(Path::new),
            )?;
        }
        Ok(())
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
        self.validate_bound_session(binding.as_ref())?;
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
                native_session_key: binding
                    .as_ref()
                    .map(|binding| binding.client_session_key.as_str()),
                native_session_file: binding
                    .as_ref()
                    .and_then(|binding| binding.client_session_file.as_deref())
                    .map(Path::new),
            });
        }
        self.start_in_process(root, request, count)
    }

    async fn exit_process_fingerprint(
        &self,
        target: &ControlTarget,
    ) -> Result<Option<pontia_runtime::ProcessFingerprint>> {
        // Only signal a dedicated process whose observed death confirms Session exit.
        let Some(observation) = self.spec.adapter.lifecycle.process_observation else {
            return Ok(None);
        };
        if observation.exit_event != crate::PontiaEventType::SessionExited
            || Some(observation.role) != target.control_runtime_role
            || !self.spec.adapter.lifecycle.coupled_runtime
        {
            return Ok(None);
        }
        let runtime = SqliteSessionRuntimeRepository::new(self.pool.clone())
            .get(target.instance()?)
            .await?;
        Ok(runtime
            .filter(|runtime| runtime.state == "running")
            .and_then(|runtime| runtime.process_fingerprint)
            .map(|value| serde_json::from_str(&value))
            .transpose()?)
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
