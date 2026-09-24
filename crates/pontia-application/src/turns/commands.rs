use pontia_core::{Error, Result, ids::new_turn_id};
use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use super::InputIntent;
use crate::{
    EventIngestService, ExternalQueryService, PontiaEvent, PontiaEventSource, PontiaEventType,
    TurnView,
    control::{ControlResult, InputReceipt},
    runtime::ControlTarget,
};

#[derive(Clone)]
pub struct TurnCommandService {
    pub(super) pool: SqlitePool,
    pub(super) event_ingest: EventIngestService,
    pub(super) queries: ExternalQueryService,
    pub(super) clients: crate::clients::ClientExecutionService,
    pub(super) scheduler: crate::inbox::InboxScheduler,
}

impl TurnCommandService {
    pub(crate) fn new(
        pool: SqlitePool,
        event_ingest: EventIngestService,
        queries: ExternalQueryService,
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

    pub async fn create_and_dispatch_turn(
        &self,
        session_id: &str,
        input: String,
        metadata: Value,
    ) -> Result<Option<TurnView>> {
        let (turn, result) = self
            .submit_input(session_id, input, metadata, InputIntent::Start)
            .await?;
        result.into_result()?;
        Ok(turn)
    }

    pub(crate) async fn submit_input(
        &self,
        session_id: &str,
        input: String,
        metadata: Value,
        intent: InputIntent,
    ) -> Result<(Option<TurnView>, ControlResult<InputReceipt>)> {
        if input.trim().is_empty() {
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
        match &intent {
            InputIntent::Start => {
                if !matches!(session.state.as_str(), "idle" | "interrupted")
                    && !(session.state == "starting" && adapter.client_owns_turn())
                    && !(session.state == "created" && adapter.prepares_on_input())
                {
                    return Err(if session.state == "busy" {
                        Error::Conflict {
                            code: "input_busy",
                            message: "Session became busy before input was submitted".into(),
                        }
                    } else {
                        Error::StateConflict(format!(
                            "session {session_id} in state {} cannot accept a new turn",
                            session.state
                        ))
                    });
                }
                if let Some(active) = active {
                    return Err(Error::Conflict {
                        code: "input_busy",
                        message: format!(
                            "session {session_id} already has active turn {}",
                            active.turn_id
                        ),
                    });
                }
            }
            InputIntent::Steer { turn_id } => {
                if !adapter.supports_steer() {
                    return Ok((
                        None,
                        ControlResult::Unsupported("This client does not support steer".into()),
                    ));
                }
                if active.as_ref().map(|turn| &turn.turn_id) != Some(turn_id) {
                    return Err(Error::StateConflict(
                        "steer target is not the current Turn".into(),
                    ));
                }
            }
        }
        if !session.capabilities.accept_task {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime cannot accept tasks"
            )));
        }
        let target = ControlTarget::resolve(&self.pool, session_id, None).await?;
        let turn_id = self
            .prepare_initial(session_id, &input, &metadata)
            .await?
            .map(|turn| turn.turn_id);
        let receipt = adapter.input(&target, input, &metadata, &intent).await;
        if let Some(turn_id) = turn_id {
            if let ControlResult::Rejected(error) = &receipt {
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
            let mut turn = query
                .get_turn(session_id, &turn_id)
                .await?
                .ok_or_else(|| Error::Domain("submitted turn missing".into()))?;
            query.enrich_turn_view(&mut turn).await?;
            return Ok((Some(turn), receipt));
        }
        Ok((None, receipt))
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

    pub(crate) async fn dispatch_initial(
        &self,
        target: &ControlTarget,
        input: &str,
        metadata: &Value,
        turn_id: Option<&str>,
    ) -> Result<()> {
        let session = target.session_id.as_str();
        let query = &self.queries;
        let session_view = query
            .get_session_control(session)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session} not found")))?;
        let adapter = self.clients.for_client(&session_view.client_type)?;
        adapter.await_initial_ready(target).await?;
        let session_view = query
            .get_session_control(session)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session} not found")))?;
        if !matches!(session_view.state.as_str(), "idle" | "interrupted") {
            return Err(Error::StateConflict(
                "Session is no longer ready for its first input".into(),
            ));
        }
        if !session_view.capabilities.accept_task {
            return Err(Error::CapabilityUnavailable(
                "Session cannot accept input".into(),
            ));
        }
        let active = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session)
            .await?;
        if active.as_ref().map(|turn| turn.turn_id.as_str()) != turn_id {
            return Err(Error::StateConflict(
                "Session has another active Turn".into(),
            ));
        }
        self.scheduler.begin_initial(session);
        let result = adapter
            .input(target, input.into(), metadata, &InputIntent::Start)
            .await;
        if matches!(
            result,
            ControlResult::Rejected(_) | ControlResult::Unsupported(_)
        ) {
            self.scheduler.finish_initial(session);
            self.scheduler.wake(session.into());
        }
        if let (Some(turn), ControlResult::Rejected(error)) = (turn_id, &result) {
            self.event_ingest
                .ingest_pontia_event(PontiaEvent::new(
                    session,
                    Some(turn.into()),
                    PontiaEventSource::RuntimeManager,
                    &session_view.client_type,
                    PontiaEventType::TurnDispatchFailed,
                    json!({"failure":{"message":error.to_string()}}),
                ))
                .await?;
        }
        result.into_result().map(|_| ())
    }
}
