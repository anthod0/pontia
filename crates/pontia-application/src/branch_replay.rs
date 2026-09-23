use pontia_core::{domain::TurnState, error::Error};
use pontia_storage_sqlite::repositories::{
    agent_bindings::SqliteAgentBindingRepository, inbox::SqliteInboxRepository,
    runtime_bindings::SqliteRuntimeBindingRepository, turns::SqliteTurnRepository,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::ExternalQueryService;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ResolveBranchReplayRequest {
    pub inbox_message_id: String,
    pub session_id: String,
    pub runtime_instance_id: String,
    pub client_type: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedBranchReplay {
    pub inbox_message_id: String,
    pub session_id: String,
    pub runtime_instance_id: String,
    pub client_type: String,
    pub replacement_input: String,
    pub target_entry_id: String,
}

#[derive(Clone)]
pub struct BranchReplayService {
    clients: crate::clients::ClientRegistry,
    pool: SqlitePool,
}

impl BranchReplayService {
    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            clients: Default::default(),
        }
    }

    pub(crate) async fn dispatch(
        &self,
        events: crate::EventIngestService,
        session: &str,
        message: &str,
    ) -> pontia_core::Result<crate::control::ControlResult<crate::control::InputReceipt>> {
        let view = ExternalQueryService::new(self.pool.clone())
            .with_clients(self.clients.clone())
            .get_session(session)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session} not found")))?;
        let target =
            crate::runtime::control_target::ControlTarget::resolve(&self.pool, session, None)
                .await?;
        self.resolve_command(ResolveBranchReplayRequest {
            inbox_message_id: message.into(),
            session_id: session.into(),
            runtime_instance_id: target.instance()?.into(),
            client_type: view.client_type.clone(),
        })
        .await?;
        let result = crate::clients::ClientAdapter::new(
            &view.client_type,
            events.clone(),
            events.client_control(),
        )?
        .replay(&target, message)
        .await;
        Ok(match result {
            crate::control::ControlResult::Sent(()) => {
                crate::control::ControlResult::Sent(crate::control::InputReceipt::default())
            }
            other => crate::control::ControlResult::from_result(
                other
                    .into_result()
                    .map(|_| crate::control::InputReceipt::default()),
            ),
        })
    }

    pub async fn validate_submission(
        &self,
        session_id: &str,
        target_turn_id: &str,
    ) -> pontia_core::Result<()> {
        self.resolve_target(session_id, target_turn_id).await?;
        Ok(())
    }

    pub async fn resolve_command(
        &self,
        request: ResolveBranchReplayRequest,
    ) -> pontia_core::Result<ResolvedBranchReplay> {
        let runtime_instance_id = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .runtime_instance_id(&request.session_id)
            .await?
            .ok_or_else(|| {
                Error::StateConflict(format!(
                    "Session {} has no bound Runtime instance",
                    request.session_id
                ))
            })?;
        if runtime_instance_id != request.runtime_instance_id {
            return Err(Error::StateConflict(format!(
                "Runtime instance does not own Session {}",
                request.session_id
            )));
        }
        let binding = SqliteAgentBindingRepository::new(self.pool.clone())
            .binding_for_session(&request.session_id)
            .await?
            .ok_or_else(|| {
                Error::StateConflict(format!(
                    "Session {} has no Agent binding",
                    request.session_id
                ))
            })?;
        if binding.client_type != request.client_type {
            return Err(Error::StateConflict(
                "Agent binding client type does not match branch replay request".to_string(),
            ));
        }
        let message = SqliteInboxRepository::new(self.pool.clone())
            .get_message(&request.session_id, &request.inbox_message_id)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "inbox message {} not found",
                    request.inbox_message_id
                ))
            })?;
        if !matches!(message.state.as_str(), "dispatching" | "dispatched") {
            return Err(Error::StateConflict(format!(
                "inbox message {} is not being delivered",
                request.inbox_message_id
            )));
        }
        let target_turn_id = message.branch_target_turn_id.as_deref().ok_or_else(|| {
            Error::Domain(format!(
                "inbox message {} is not a branch submission",
                request.inbox_message_id
            ))
        })?;
        let target_entry_id = self
            .resolve_target(&request.session_id, target_turn_id)
            .await?;

        Ok(ResolvedBranchReplay {
            inbox_message_id: request.inbox_message_id,
            session_id: request.session_id,
            runtime_instance_id: request.runtime_instance_id,
            client_type: request.client_type,
            replacement_input: message.input_summary,
            target_entry_id,
        })
    }

    async fn resolve_target(
        &self,
        session_id: &str,
        target_turn_id: &str,
    ) -> pontia_core::Result<String> {
        let query = ExternalQueryService::new(self.pool.clone()).with_clients(self.clients.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if !session.capabilities.branch_control {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} does not support branch control"
            )));
        }
        if !matches!(session.state.as_str(), "idle" | "interrupted" | "exited") {
            return Err(Error::StateConflict(format!(
                "session {session_id} in state {} cannot navigate branches",
                session.state
            )));
        }

        let turns = SqliteTurnRepository::new(self.pool.clone());
        if let Some(active_turn) = turns.active_turn(session_id).await? {
            return Err(Error::StateConflict(format!(
                "session {session_id} has active Turn {}",
                active_turn.turn_id
            )));
        }
        let target = turns
            .get_projection(target_turn_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("turn {target_turn_id} not found")))?;
        if target.session_id != session_id {
            return Err(Error::StateConflict(format!(
                "Turn {target_turn_id} belongs to another Session"
            )));
        }
        let target_state = target.state.parse::<TurnState>()?;
        if !target_state.is_terminal()
            || target
                .input_summary
                .as_deref()
                .is_none_or(|input| input.trim().is_empty())
        {
            return Err(Error::StateConflict(format!(
                "Turn {target_turn_id} is not eligible for branch replay"
            )));
        }

        let binding = SqliteAgentBindingRepository::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
            .ok_or_else(|| {
                Error::StateConflict(format!("Session {session_id} has no Agent binding"))
            })?;
        if binding.client_type != session.client_type {
            return Err(Error::StateConflict(
                "Agent binding does not match Session client".into(),
            ));
        }
        let all_turns = turns.list_turns(session_id).await?;
        let is_first_session_turn = all_turns
            .first()
            .is_some_and(|turn| turn.turn_id == target_turn_id);
        crate::clients::ClientAdapter::new(
            &session.client_type,
            crate::EventIngestService::new(self.pool.clone()).with_clients(self.clients.clone()),
            None,
        )?
        .branch_target(binding, target, is_first_session_turn)
    }
}
