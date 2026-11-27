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
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::{
    ControlQueryService, InboxMessageView, TurnCommandService,
    views::inbox::{cancelled_view, dispatched_view, row_to_view},
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SubmitInboxMessageRequest {
    pub input: String,
    #[serde(default = "default_delivery_policy")]
    pub delivery_policy: String,
    #[serde(default)]
    pub metadata: Value,
}

fn default_delivery_policy() -> String {
    "after_idle".to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct InboxCommandOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct InboxCommandService {
    pool: SqlitePool,
    queries: ControlQueryService,
    clients: crate::clients::ClientExecutionService,
    turns: TurnCommandService,
    pub(crate) scheduler: InboxScheduler,
}

struct EnqueueOptions {
    interrupt_before_delivery: bool,
    drain_now: bool,
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

    pub async fn submit_message(
        &self,
        session_id: &str,
        request: SubmitInboxMessageRequest,
    ) -> Result<InboxCommandOutcome> {
        self.submit_message_with_id(&new_message_id().to_string(), session_id, request)
            .await
    }

    /// Queues a message under a caller-owned stable identity. Repeating that
    /// identity for the same Session returns the current record; the repeated
    /// request does not replace its contents or enqueue a second message.
    pub async fn submit_message_once(
        &self,
        message_id: &str,
        session_id: &str,
        request: SubmitInboxMessageRequest,
    ) -> Result<InboxCommandOutcome> {
        self.submit_message_with_id(message_id, session_id, request)
            .await
    }

    async fn submit_message_with_id(
        &self,
        message_id: &str,
        session_id: &str,
        request: SubmitInboxMessageRequest,
    ) -> Result<InboxCommandOutcome> {
        let actor = self.scheduler.session(session_id);
        let _guard = actor.lock().await;
        self.enqueue_message_locked(
            message_id,
            session_id,
            request,
            EnqueueOptions {
                interrupt_before_delivery: true,
                drain_now: true,
            },
            &actor,
        )
        .await
    }

    async fn enqueue_message_locked(
        &self,
        message_id: &str,
        session_id: &str,
        request: SubmitInboxMessageRequest,
        options: EnqueueOptions,
        actor: &scheduling::SessionActor,
    ) -> Result<InboxCommandOutcome> {
        if message_id.is_empty() || message_id.len() > 512 {
            return Err(Error::Domain("Invalid Inbox submission identity".into()));
        }
        let inbox_repository = SqliteInboxRepository::new(self.pool.clone());
        if let Some(existing) = inbox_repository.get_message(session_id, message_id).await? {
            return Ok(InboxCommandOutcome {
                data: json!({"inbox_message":row_to_view(existing)?}),
                duplicate: true,
            });
        }
        if request.input.trim().is_empty() {
            return Err(Error::Domain(
                "inbox message input must not be blank".to_string(),
            ));
        }
        if !matches!(
            request.delivery_policy.as_str(),
            "after_idle" | "interrupt_now" | "steer"
        ) {
            return Err(Error::Domain(format!(
                "unknown delivery_policy: {}",
                request.delivery_policy
            )));
        }

        let query = &self.queries;
        let session = query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let metadata = serde_json::to_string(&request.metadata)?;

        let inserted = inbox_repository
            .enqueue(
                pontia_storage_sqlite::repositories::inbox::NewInboxMessage {
                    message_id,
                    session_id,
                    input: &request.input,
                    metadata: &metadata,
                },
            )
            .await?;
        if !inserted {
            return Err(Error::StateConflict(
                "Submission identity belongs to another Session".into(),
            ));
        }
        let queued_message = self
            .get_message(session_id, message_id)
            .await?
            .ok_or_else(|| Error::Domain("queued inbox message missing".to_string()))?;

        let active_turn = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?;
        if options.interrupt_before_delivery
            && request.delivery_policy == "interrupt_now"
            && active_turn.is_some()
        {
            if !session.capabilities.interrupt {
                inbox_repository.remove(session_id, message_id).await?;
                return Err(Error::CapabilityUnavailable(
                    "session ".to_string() + session_id + " runtime does not support interrupt",
                ));
            } else if let Err(error) = self
                .turns
                .interrupt_current_turn_locked(session_id, actor)
                .await
            {
                inbox_repository.remove(session_id, message_id).await?;
                return Err(error);
            }
        }

        if options.drain_now {
            self.drain_inbox_locked(session_id, actor).await?;
        }

        let message = match self.get_message(session_id, message_id).await? {
            Some(message) => serde_json::to_value(message)?,
            None => {
                let turn_id = SqliteTurnRepository::new(self.pool.clone())
                    .active_turn(session_id)
                    .await?
                    .map(|turn| turn.turn_id);
                serde_json::to_value(dispatched_view(queued_message, turn_id))?
            }
        };
        Ok(InboxCommandOutcome {
            data: json!({ "inbox_message": message }),
            duplicate: false,
        })
    }

    pub async fn list_messages(&self, session_id: &str) -> Result<Vec<InboxMessageView>> {
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
    ) -> Result<Option<InboxMessageView>> {
        let row = SqliteInboxRepository::new(self.pool.clone())
            .get_message(session_id, message_id)
            .await?;
        row.map(row_to_view).transpose()
    }

    pub async fn cancel_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<InboxCommandOutcome> {
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
            .ok_or_else(|| Error::NotFound(format!("inbox message {message_id} not found")))?;
        SqliteInboxRepository::new(self.pool.clone())
            .remove(session_id, message_id)
            .await?;
        self.notify_available(session_id);
        Ok(InboxCommandOutcome {
            data: json!({ "inbox_message": cancelled_view(message) }),
            duplicate: false,
        })
    }

    pub async fn dismiss_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<InboxCommandOutcome> {
        let query = &self.queries;
        query
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        Err(Error::NotFound(format!(
            "inbox message {message_id} not found"
        )))
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
