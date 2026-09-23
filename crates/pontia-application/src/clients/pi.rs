use super::ClientAdapter;
use crate::{RuntimeReadinessService, runtime::control_target::ControlTarget};
use pontia_core::{Error, Result};
use pontia_runtime::GenericRuntimeManager;

impl ClientAdapter {
    pub(super) async fn pi_input(
        &self,
        target: &ControlTarget,
        input: &str,
        message: Option<&str>,
    ) -> Result<()> {
        RuntimeReadinessService::new(self.events.db())
            .wait_until_ready(
                &target.session_id,
                self.spec.client_type,
                target.instance()?,
            )
            .await?;
        target.validate(&self.events.db()).await?;
        self.pi
            .as_ref()
            .ok_or_else(|| {
                Error::CapabilityUnavailable("Pi control service is unavailable".into())
            })?
            .submit(&target.session_id, target.instance()?, input, message)
            .await
    }

    pub(super) async fn pi_interrupt(&self, target: &ControlTarget) -> Result<()> {
        let (socket, pane) = target.tmux_pane(&self.events.db()).await?;
        GenericRuntimeManager
            .interrupt_session(&socket, &pane, self.spec.adapter.interrupt)
            .map_err(|error| Error::ControlUnknown(error.to_string()))
    }

    pub async fn replay(
        &self,
        target: &ControlTarget,
        message: &str,
    ) -> crate::control::ControlResult<()> {
        if !self.spec.capabilities.branch_control {
            return crate::control::ControlResult::Unsupported(
                "client does not support branch control".into(),
            );
        }
        let result = async {
            RuntimeReadinessService::new(self.events.db())
                .wait_until_ready(
                    &target.session_id,
                    self.spec.client_type,
                    target.instance()?,
                )
                .await?;
            target.validate(&self.events.db()).await?;
            self.pi
                .as_ref()
                .ok_or_else(|| {
                    Error::CapabilityUnavailable("Pi control service is unavailable".into())
                })?
                .replay(&target.session_id, target.instance()?, message)
                .await
        }
        .await;
        match result {
            Ok(()) => crate::control::ControlResult::Sent(()),
            Err(error) => crate::control::ControlResult::from_result(Err(error)),
        }
    }
}

impl ClientAdapter {
    pub fn branch_target(
        &self,
        binding: pontia_storage_sqlite::models::agent_bindings::AgentBindingRow,
        target: pontia_storage_sqlite::models::turns::TurnProjectionRow,
        is_first_session_turn: bool,
    ) -> Result<String> {
        use pontia_agent_clients::{
            pi::raw_transcripts::{
                PiTimelineAdapter, PiTurnUserEntryResolveRequest, PiTurnUserEntryResolver,
            },
            raw_transcripts::AgentBindingResolveRequest,
        };
        if !self.spec.capabilities.branch_control {
            return Err(Error::CapabilityUnavailable(
                "client does not support branch control".into(),
            ));
        }
        let backend = pontia_agent_clients::turn_timeline_backend_for(self.spec.client_type)
            .ok_or_else(|| {
                Error::CapabilityUnavailable("branch target source unavailable".into())
            })?;
        let source = backend
            .resolver
            .resolve(&AgentBindingResolveRequest {
                id: binding.id,
                session_id: binding.session_id.clone(),
                client_type: binding.client_type,
                client_session_file: binding.client_session_file.map(Into::into),
            })
            .map_err(|error| {
                Error::StateConflict(format!("Pi branch target source unavailable: {error}"))
            })?;
        PiTimelineAdapter::new()
            .resolve_user_entry(PiTurnUserEntryResolveRequest {
                source,
                session_id: binding.session_id,
                turn_session_id: target.session_id,
                turn_id: target.turn_id,
                is_first_session_turn,
                head_cursor: target.head_cursor,
                tail_cursor: target.tail_cursor,
            })
            .map(|resolved| resolved.entry_id)
            .map_err(|error| Error::StateConflict(error.to_string()))
    }
}
