use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use pontia_core::{domain::TurnState, error::Error, ids::new_branch_replay_id};
use pontia_storage_sqlite::repositories::{
    agent_bindings::SqliteAgentBindingRepository, turns::SqliteTurnRepository,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{ControlQueryService, runtime::ControlTarget};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubmitBranchReplayRequest {
    pub target_turn_id: String,
    pub input: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BranchReplayOutcome {
    pub replay_id: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ResolveBranchReplayRequest {
    pub replay_id: String,
    pub session_id: String,
    pub runtime_id: String,
    pub client_type: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedBranchReplay {
    pub replay_id: String,
    pub session_id: String,
    pub runtime_id: String,
    pub client_type: String,
    pub replacement_input: String,
    pub target_entry_id: String,
}

#[derive(Clone)]
pub struct BranchReplayService {
    clients: crate::clients::ClientRegistry,
    execution: crate::clients::ClientExecutionService,
    pending: Arc<Mutex<HashMap<String, ResolvedBranchReplay>>>,
    pool: SqlitePool,
    scheduler: crate::inbox::InboxScheduler,
}

impl BranchReplayService {
    pub(crate) fn new(
        pool: SqlitePool,
        clients: crate::clients::ClientRegistry,
        execution: crate::clients::ClientExecutionService,
        scheduler: crate::inbox::InboxScheduler,
    ) -> Self {
        Self {
            pool,
            clients,
            execution,
            pending: Default::default(),
            scheduler,
        }
    }

    pub async fn submit(
        &self,
        session_id: &str,
        target_turn_id: &str,
        input: &str,
    ) -> pontia_core::Result<BranchReplayOutcome> {
        let actor = self.scheduler.session(session_id);
        let _guard = actor.lock().await;
        if input.trim().is_empty() {
            return Err(Error::Domain(
                "branch replay input must not be blank".into(),
            ));
        }

        let session = ControlQueryService::new(self.pool.clone())
            .with_clients(self.clients.clone())
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if session.state != "idle" {
            return Err(Error::StateConflict(format!(
                "session {session_id} in state {} cannot replay a branch",
                session.state
            )));
        }
        let target_entry_id = self.resolve_target(session_id, target_turn_id).await?;
        let target = ControlTarget::resolve(&self.pool, &self.clients, session_id, None).await?;
        let runtime_id = target.instance()?.to_string();
        let channel = self.execution.for_client(&session.client_type)?;
        let replay_id = new_branch_replay_id().to_string();
        let replay = ResolvedBranchReplay {
            replay_id: replay_id.clone(),
            session_id: session_id.to_string(),
            runtime_id,
            client_type: session.client_type.clone(),
            replacement_input: input.to_string(),
            target_entry_id,
        };
        self.pending
            .lock()
            .expect("branch replay lock")
            .insert(replay_id.clone(), replay);

        let delivery = channel.replay(&target, &replay_id).await;
        if let Err(error) = delivery.into_result() {
            self.pending
                .lock()
                .expect("branch replay lock")
                .remove(&replay_id);
            return Err(error);
        }
        Ok(BranchReplayOutcome { replay_id })
    }

    pub async fn resolve_command(
        &self,
        request: ResolveBranchReplayRequest,
    ) -> pontia_core::Result<ResolvedBranchReplay> {
        let mut pending = self.pending.lock().expect("branch replay lock");
        let replay = pending.get(&request.replay_id).ok_or_else(|| {
            Error::NotFound(format!("branch replay {} not found", request.replay_id))
        })?;
        if replay.session_id != request.session_id
            || replay.runtime_id != request.runtime_id
            || replay.client_type != request.client_type
        {
            return Err(Error::StateConflict(
                "branch replay does not match its connection identity".into(),
            ));
        }
        Ok(pending
            .remove(&request.replay_id)
            .expect("branch replay was checked above"))
    }

    async fn resolve_target(
        &self,
        session_id: &str,
        target_turn_id: &str,
    ) -> pontia_core::Result<String> {
        let query = ControlQueryService::new(self.pool.clone()).with_clients(self.clients.clone());
        let session = query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        session.require_capability(|caps| caps.branch_control, "branch control")?;

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
        if let Some(history) = self
            .clients
            .get(&session.client_type)
            .and_then(|registration| {
                registration.session.as_ref()?.native_history(
                    self.pool.clone(),
                    crate::client_contract::raw_transcripts::AgentBindingResolveRequest {
                        id: binding.id.clone(),
                        session_id: binding.session_id.clone(),
                        client_type: binding.client_type.clone(),
                        client_session_key: binding.client_session_key.clone(),
                        client_session_file: binding.client_session_file.clone().map(Into::into),
                    },
                )
            })
        {
            return history
                .branch_target(crate::client_contract::raw_transcripts::TurnTimelineRange {
                    turn_id: target.turn_id.clone(),
                    is_first_session_turn,
                    head_cursor: target
                        .head_cursor
                        .clone()
                        .ok_or_else(|| Error::StateConflict("branch target head missing".into()))?,
                    tail_cursor: target.tail_cursor.clone(),
                })
                .await;
        }
        self.clients
            .data(&session.client_type)
            .ok_or_else(|| Error::Unsupported("branch target source unavailable".into()))?
            .branch_target(crate::client_contract::BranchTargetRequest {
                binding,
                turn: target,
                is_first_session_turn,
            })
    }
}
