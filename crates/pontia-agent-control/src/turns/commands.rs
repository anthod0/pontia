use pontia_core::{Error, Result, ids::new_turn_id};
use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::{
    ControlQueryService, EventIngestService, InputPayload, PontiaEvent, PontiaEventSource,
    PontiaEventType, TurnView,
    control::{ControlResult, InputReceipt},
    runtime::ControlTarget,
};

#[derive(Clone)]
pub struct TurnCommandService {
    pub(super) pool: SqlitePool,
    pub(super) event_ingest: EventIngestService,
    pub(super) queries: ControlQueryService,
    pub(super) clients: crate::clients::ClientExecutionService,
    pub(super) scheduler: crate::inbox::InboxScheduler,
}

impl TurnCommandService {
    pub(crate) fn new(
        pool: SqlitePool,
        event_ingest: EventIngestService,
        queries: ControlQueryService,
        clients: crate::clients::ClientExecutionService,
        scheduler: crate::inbox::InboxScheduler,
    ) -> Self {
        Self {
            pool,
            event_ingest,
            queries,
            clients,
            scheduler,
        }
    }

    pub async fn start_turn(
        &self,
        session_id: &str,
        payload: InputPayload,
        metadata: Value,
    ) -> Result<Option<TurnView>> {
        let actor = self.scheduler.session(session_id);
        let _guard = actor.lock().await;
        actor.set_phase(crate::inbox::SessionPhase::StartingTurn);
        let (turn_id, result) = match self
            .submit_start_turn(session_id, payload, metadata, None, None)
            .await
        {
            Ok(delivery) => delivery,
            Err(error) => {
                actor.cancel_start();
                return Err(error);
            }
        };
        if let Err(error) = result.into_result() {
            actor.cancel_start();
            return Err(error);
        }
        actor.finish_start();
        let Some(turn_id) = turn_id else {
            return Ok(None);
        };
        let mut turn = self
            .queries
            .get_turn(session_id, &turn_id)
            .await?
            .ok_or_else(|| Error::Domain("submitted turn missing".into()))?;
        self.queries.enrich_turn_view(&mut turn).await?;
        Ok(Some(turn))
    }

    pub(crate) async fn submit_start_turn(
        &self,
        session_id: &str,
        payload: InputPayload,
        metadata: Value,
        required_runtime_id: Option<&str>,
        prepared_turn_id: Option<&str>,
    ) -> Result<(Option<String>, ControlResult<InputReceipt>)> {
        if payload.text.trim().is_empty() {
            return Err(Error::Domain("input must not be blank".into()));
        }
        let query = &self.queries;
        let session = query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let adapter = self.clients.for_client(&session.client_type)?;
        let active = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?;
        if !matches!(session.state.as_str(), "idle" | "interrupted")
            && !(session.state == "starting" && adapter.client_owns_turn())
            && !(session.state == "created" && adapter.prepares_on_input())
        {
            return Err(
                if matches!(session.state.as_str(), "busy" | "waiting_for_interaction") {
                    Error::Conflict {
                        code: "input_busy",
                        message: "Session became busy before input was submitted".into(),
                    }
                } else {
                    Error::StateConflict(format!(
                        "session {session_id} in state {} cannot accept a new turn",
                        session.state
                    ))
                },
            );
        }
        if let Some(prepared_turn_id) = prepared_turn_id {
            if active.as_ref().map(|turn| turn.turn_id.as_str()) != Some(prepared_turn_id) {
                return Err(Error::StateConflict(format!(
                    "prepared Turn {prepared_turn_id} is not active for session {session_id}"
                )));
            }
        } else if let Some(active) = active {
            return Err(Error::Conflict {
                code: "input_busy",
                message: format!(
                    "session {session_id} already has active turn {}",
                    active.turn_id
                ),
            });
        }
        if !session.capabilities.accept_task {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime cannot accept tasks"
            )));
        }
        let target = ControlTarget::resolve(
            &self.pool,
            self.clients.registry(),
            session_id,
            required_runtime_id,
        )
        .await?;
        let turn_id = match prepared_turn_id {
            Some(turn_id) => Some(turn_id.to_owned()),
            None => self
                .prepare_initial(session_id, &payload.text, &metadata)
                .await?
                .map(|turn| turn.turn_id),
        };
        let receipt = adapter
            .start_turn(&target, payload, turn_id.as_deref())
            .await;
        if let Some(turn_id) = &turn_id
            && let ControlResult::Rejected(error) = &receipt
        {
            self.event_ingest
                .ingest_pontia_event(PontiaEvent::new(
                    session_id,
                    Some(turn_id.clone()),
                    PontiaEventSource::RuntimeManager,
                    &session.client_type,
                    PontiaEventType::TurnDispatchFailed,
                    json!({"failure":{"message":error.to_string()}}),
                ))
                .await?;
        }
        Ok((turn_id, receipt))
    }

    pub(crate) async fn submit_steer(
        &self,
        session_id: &str,
        payload: InputPayload,
        expected_turn_id: &str,
        required_runtime_id: Option<&str>,
    ) -> Result<(Option<String>, ControlResult<InputReceipt>)> {
        if payload.text.trim().is_empty() {
            return Err(Error::Domain("input must not be blank".into()));
        }
        let session = self
            .queries
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if !session.capabilities.steer {
            return Ok((
                None,
                ControlResult::Unsupported("This client does not support steer".into()),
            ));
        }
        let active = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?;
        if active.as_ref().map(|turn| turn.turn_id.as_str()) != Some(expected_turn_id) {
            return Err(Error::Conflict {
                code: "steer_target_changed",
                message: "steer target is not the current Turn".into(),
            });
        }
        if !session.capabilities.accept_task {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime cannot accept tasks"
            )));
        }
        let target = ControlTarget::resolve(
            &self.pool,
            self.clients.registry(),
            session_id,
            required_runtime_id,
        )
        .await?;
        let receipt = self
            .clients
            .for_client(&session.client_type)?
            .steer(&target, payload)
            .await;
        let pending = matches!(&receipt, ControlResult::Accepted(r) | ControlResult::Sent(r) if r.turn_pending);
        Ok(((!pending).then(|| expected_turn_id.to_owned()), receipt))
    }

    pub(crate) async fn prepare_initial(
        &self,
        session_id: &str,
        input: &str,
        metadata: &Value,
    ) -> Result<Option<TurnView>> {
        if input.trim().is_empty() {
            return Err(Error::Domain("input must not be blank".into()));
        }
        let query = &self.queries;
        let session = query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let adapter = self.clients.for_client(&session.client_type)?;
        if adapter.client_owns_turn() {
            return Ok(None);
        }
        let turn_id = new_turn_id().to_string();
        for (kind, payload) in [
            (
                PontiaEventType::TurnCreated,
                json!({"input":{"summary":input},"metadata":metadata}),
            ),
            (PontiaEventType::TurnQueued, json!({})),
        ] {
            self.event_ingest
                .ingest_pontia_event(PontiaEvent::new(
                    session_id,
                    Some(turn_id.clone()),
                    PontiaEventSource::ExternalApi,
                    &session.client_type,
                    kind,
                    payload,
                ))
                .await?;
        }
        query.get_turn(session_id, &turn_id).await
    }
}
