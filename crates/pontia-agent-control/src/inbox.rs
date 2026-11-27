mod live;
pub use live::{SendMode, SendOutcome};
mod scheduling;
pub(crate) use scheduling::{InboxScheduler, SessionActor, SessionPhase};

use pontia_core::{
    error::{Error, Result},
    ids::new_message_id,
};
use pontia_storage_sqlite::repositories::{
    inbox::SqliteInboxRepository, turns::SqliteTurnRepository,
};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::{
    ControlQueryService, InputPayload, QueuedMessageView, TurnCommandService,
    views::inbox::row_to_view,
};

#[derive(Clone)]
pub struct InboxCommandService {
    pool: SqlitePool,
    queries: ControlQueryService,
    clients: crate::clients::ClientExecutionService,
    turns: TurnCommandService,
    pub(crate) scheduler: InboxScheduler,
}

impl InboxCommandService {
    pub(crate) fn new(
        pool: SqlitePool,
        queries: ControlQueryService,
        clients: crate::clients::ClientExecutionService,
        turns: TurnCommandService,
        scheduler: InboxScheduler,
    ) -> Self {
        Self {
            pool,
            queries,
            clients,
            turns,
            scheduler,
        }
    }

    async fn enqueue_message_locked(
        &self,
        message_id: &str,
        session_id: &str,
        input: InputPayload,
    ) -> Result<()> {
        let inbox_repository = SqliteInboxRepository::new(self.pool.clone());
        if input.text.trim().is_empty() {
            return Err(Error::Domain("message input must not be blank".to_string()));
        }

        let query = &self.queries;
        query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let metadata = serde_json::to_string(&input.metadata)?;

        let inserted = inbox_repository
            .enqueue(
                pontia_storage_sqlite::repositories::inbox::NewInboxMessage {
                    message_id,
                    session_id,
                    input: &input.text,
                    metadata: &metadata,
                },
            )
            .await?;
        if !inserted {
            return Err(Error::StateConflict(
                "message identity already exists".into(),
            ));
        }

        Ok(())
    }

    pub async fn list_messages(&self, session_id: &str) -> Result<Vec<QueuedMessageView>> {
        self.queries
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let rows = SqliteInboxRepository::new(self.pool.clone())
            .list_messages(session_id)
            .await?;
        rows.into_iter().map(row_to_view).collect()
    }

    pub async fn get_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<Option<QueuedMessageView>> {
        let row = SqliteInboxRepository::new(self.pool.clone())
            .get_message(session_id, message_id)
            .await?;
        row.map(row_to_view).transpose()
    }

    pub async fn cancel_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<QueuedMessageView> {
        let query = &self.queries;
        query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        let actor = self.scheduler.session(session_id);
        let _guard = actor.lock().await;
        let message = self
            .get_message(session_id, message_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("queued message {message_id} not found")))?;
        SqliteInboxRepository::new(self.pool.clone())
            .remove(session_id, message_id)
            .await?;
        self.notify_available(session_id);
        Ok(message)
    }

    pub async fn drain_inbox(&self, session_id: &str) -> Result<()> {
        let actor = self.scheduler.session(session_id);
        let _guard = actor.lock().await;
        self.drain_inbox_locked(session_id, &actor).await
    }

    async fn drain_inbox_locked(
        &self,
        session_id: &str,
        actor: &scheduling::SessionActor,
    ) -> Result<()> {
        let query = &self.queries;
        let Some(session) = query.get_session_control(session_id).await? else {
            return Ok(());
        };
        let adapter = self.clients.for_client(&session.client_type)?;
        let active = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?;
        let inbox_repository = SqliteInboxRepository::new(self.pool.clone());
        let Some(row) = inbox_repository.next_message(session_id).await? else {
            return Ok(());
        };
        if !adapter.input_available(session_id).await? {
            return Ok(());
        }
        if matches!(session.state.as_str(), "exited" | "error") {
            return Ok(());
        }
        let prepared_turn_id = active
            .as_ref()
            .filter(|turn| turn.state == "queued")
            .map(|turn| turn.turn_id.as_str());
        if active.is_some() && prepared_turn_id.is_none() {
            return Ok(());
        }
        if actor.phase() != scheduling::SessionPhase::Idle {
            return Ok(());
        }
        if !matches!(session.state.as_str(), "idle" | "interrupted")
            && !(matches!(session.state.as_str(), "created" | "starting")
                && adapter.prepares_on_input())
        {
            return Ok(());
        }
        let message_id = row.message_id;
        let input = row.input_summary;
        let metadata = row.metadata;
        let mut metadata: Value = serde_json::from_str(&metadata)?;
        if !metadata.is_object() {
            metadata = json!({});
        }
        actor.set_phase(scheduling::SessionPhase::StartingTurn);
        let delivery = match self
            .turns
            .submit_start_turn(session_id, input.into(), metadata, None, prepared_turn_id)
            .await
        {
            Ok((turn_id, result)) => result
                .into_delivery_result()
                .map(|receipt| (turn_id, receipt)),
            Err(error) => Err(crate::DeliveryError::from(error)),
        };
        match delivery {
            Ok((_turn_id, _receipt)) => {
                actor.finish_start();
                inbox_repository.remove(session_id, &message_id).await?;
                self.notify_available(session_id);
            }
            Err(crate::DeliveryError::Offline(_)) => {
                actor.cancel_start();
            }
            Err(crate::DeliveryError::StateChanged(_)) => {
                actor.cancel_start();
            }
            Err(error) => {
                actor.cancel_start();
                inbox_repository.remove(session_id, &message_id).await?;
                self.notify_available(session_id);
                return Err(error.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
