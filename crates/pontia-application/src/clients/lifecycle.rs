use super::ClientAdapter;
use crate::{control::ControlResult, runtime::control_target::ControlTarget};
use pontia_agent_clients::TerminateBehavior;
use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult};
use std::path::Path;

impl ClientAdapter {
    pub async fn start(
        &self,
        root: &Path,
        request: RuntimeStartRequest,
    ) -> Result<Option<RuntimeStartResult>> {
        if self.prepares_on_input() {
            let cwd = request
                .workspace
                .as_deref()
                .map(std::path::PathBuf::from)
                .unwrap_or(std::env::current_dir()?);
            crate::codex::CodexService::new(self.events.clone())
                .provision(&request.session_id, root, &cwd)
                .await?;
            sqlx::query("UPDATE runtime_bindings SET adapter_details=json_set(adapter_details,'$.codex_environment',json(?)) WHERE session_id=?")
                .bind(serde_json::to_string(&request.environment)?).bind(&request.session_id).execute(&self.events.db()).await?;
            return Ok(None);
        }
        GenericRuntimeManager.start_session(root, request).map(Some)
    }

    pub async fn exit(&self, target: &ControlTarget) -> ControlResult<()> {
        let result = async {
            target.validate(&self.events.db()).await?;
            match self.spec.adapter.terminate {
                TerminateBehavior::CodexArchive => crate::codex::CodexService::new(self.events.clone()).archive(target).await,
                TerminateBehavior::TmuxSendKeys(keys) => {
                    let (socket, pane) = target.tmux_pane(&self.events.db()).await?;
                    GenericRuntimeManager.send_tmux_keys(&socket, &pane, keys).map_err(|error| Error::ControlUnknown(error.to_string()))
                }
                TerminateBehavior::RuntimeManager => {
                    if let Some(handle) = pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository::new(self.events.db()).runtime_handle(&target.session_id).await? {
                        GenericRuntimeManager.terminate_session(&handle)?;
                    }
                    Ok(())
                }
            }
        }.await;
        match result {
            Ok(())
                if matches!(
                    self.spec.adapter.terminate,
                    TerminateBehavior::TmuxSendKeys(_)
                ) =>
            {
                ControlResult::Sent(())
            }
            other => ControlResult::from_result(other),
        }
    }

    pub async fn resume(
        &self,
        target: &ControlTarget,
        root: &Path,
        mut request: RuntimeStartRequest,
        count: i64,
    ) -> Result<Option<RuntimeStartResult>> {
        target.validate(&self.events.db()).await?;
        if self.prepares_on_input() {
            crate::codex::CodexService::new(self.events.clone())
                .resume(target)
                .await?;
            return Ok(None);
        }
        if let (Some(command), Some(argument)) = (
            request.start_command.as_ref(),
            self.spec
                .tmux_runtime()
                .and_then(|runtime| runtime.resume_session_identity_arg),
        ) && let Some(binding) = crate::AgentBindingService::new(self.events.db())
            .binding_for_session(&target.session_id)
            .await?
        {
            request.start_command = Some(format!(
                "{command} {argument} '{}'",
                binding.client_session_key.replace('\'', "'\\''")
            ));
        }
        let pane = target.tmux_pane(&self.events.db()).await.ok();
        target.validate(&self.events.db()).await?;
        GenericRuntimeManager
            .start_session_with_restart_count_and_reuse_target(
                root,
                request,
                count,
                pane.as_ref()
                    .map(|(socket, pane)| (socket.as_str(), pane.as_str())),
            )
            .map(Some)
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
        target.validate(&self.events.db()).await?;
        match self.spec.adapter.terminate {
            TerminateBehavior::TmuxSendKeys(_) => {
                let (socket, pane) = target.tmux_pane(&self.events.db()).await?;
                GenericRuntimeManager.kill_tmux_pane(&socket, &pane)?;
            }
            TerminateBehavior::RuntimeManager => {
                self.exit(target).await.into_result()?;
            }
            TerminateBehavior::CodexArchive => unreachable!(),
        }
        GenericRuntimeManager.start_session_with_restart_count(root, request, count)
    }

    pub async fn ensure_exit_available(&self, target: &ControlTarget) -> Result<()> {
        target.validate(&self.events.db()).await?;
        match self.spec.adapter.terminate {
            TerminateBehavior::TmuxSendKeys(_) => {
                target.tmux_pane(&self.events.db()).await?;
            }
            TerminateBehavior::CodexArchive => {
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
    if runtime.runtime_kind != "codex_app_server" {
        let _ = GenericRuntimeManager.terminate_session(&runtime.runtime_handle);
    }
}
