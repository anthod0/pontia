use super::TurnCommandService;
use crate::{
    ControlCommandOutcome, ExternalQueryService, PontiaEvent, PontiaEventSource, PontiaEventType,
};
use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;
use serde_json::json;
impl TurnCommandService {
    pub async fn interrupt_current_turn(&self, session_id: &str) -> Result<ControlCommandOutcome> {
        let query =
            ExternalQueryService::new(self.pool.clone()).with_clients(self.event_ingest.clients());
        if query.get_session(session_id).await?.is_none() {
            return Err(Error::NotFound(format!("session {session_id} not found")));
        }
        let active_turn = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?
            .ok_or_else(|| {
                Error::StateConflict(format!(
                    "session {session_id} has no active turn to interrupt"
                ))
            })?;
        self.interrupt_turn(session_id, &active_turn.turn_id).await
    }

    pub async fn interrupt_turn_for_runtime(
        &self,
        session_id: &str,
        turn_id: &str,
        runtime_instance_id: &str,
    ) -> Result<ControlCommandOutcome> {
        self.interrupt_turn_scoped(session_id, turn_id, Some(runtime_instance_id))
            .await
    }

    pub async fn interrupt_turn(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<ControlCommandOutcome> {
        self.interrupt_turn_scoped(session_id, turn_id, None).await
    }

    async fn interrupt_turn_scoped(
        &self,
        session_id: &str,
        turn_id: &str,
        expected: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        let target = crate::runtime::control_target::ControlTarget::resolve(
            &self.pool, session_id, expected,
        )
        .await?;
        let query =
            ExternalQueryService::new(self.pool.clone()).with_clients(self.event_ingest.clients());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let turn = query
            .get_turn(session_id, turn_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("turn {turn_id} not found")))?;

        if matches!(turn.state.as_str(), "completed" | "failed" | "interrupted") {
            return Err(Error::StateConflict(format!(
                "turn {turn_id} is already terminal"
            )));
        }
        if SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?
            .as_ref()
            .map(|turn| turn.turn_id.as_str())
            != Some(turn_id)
        {
            return Err(Error::StateConflict(format!(
                "turn {turn_id} is not the active turn for session {session_id}"
            )));
        }
        if !session.capabilities.interrupt {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime does not support interrupt"
            )));
        }
        self.interrupt_validated(&session, &turn, target).await
    }

    async fn interrupt_validated(
        &self,
        session: &crate::SessionView,
        turn: &crate::TurnView,
        target: crate::runtime::control_target::ControlTarget,
    ) -> Result<ControlCommandOutcome> {
        let session_id = &session.session_id;
        let turn_id = &turn.turn_id;
        let query =
            ExternalQueryService::new(self.pool.clone()).with_clients(self.event_ingest.clients());
        crate::clients::ClientAdapter::new(
            &session.client_type,
            self.event_ingest.clone(),
            self.client_control.clone(),
        )?
        .interrupt(&target, turn_id)
        .await
        .into_result()?;
        let ingest = self.event_ingest.clone();
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                Some(turn_id.to_string()),
                PontiaEventSource::ExternalApi,
                session.client_type.clone(),
                PontiaEventType::TurnInterruptRequested,
                json!({}),
            ))
            .await?;
        let turn = query
            .get_turn(session_id, turn_id)
            .await?
            .ok_or_else(|| Error::Domain("interrupted turn missing".to_string()))?;
        let data = json!({ "turn": turn });
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }
}
