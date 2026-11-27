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

use crate::{ControlQueryService, InboxMessageView, TurnCommandService, views::inbox::row_to_view};

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
    resuming: bool,
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
                resuming: false,
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

        let active_turn = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?;
        let delivery_policy = if request.delivery_policy == "steer" && !session.capabilities.steer {
            "after_idle"
        } else {
            request.delivery_policy.as_str()
        };
        let steer_target = if delivery_policy == "steer" {
            active_turn.as_ref().map(|turn| turn.turn_id.as_str())
        } else {
            None
        };
        let inserted = inbox_repository
            .enqueue(
                pontia_storage_sqlite::repositories::inbox::NewInboxMessage {
                    message_id,
                    session_id,
                    delivery_policy,
                    input: &request.input,
                    metadata: &metadata,
                    steer_target,
                    resuming: options.resuming,
                },
            )
            .await?;
        if !inserted {
            return Err(Error::StateConflict(
                "Submission identity belongs to another Session".into(),
            ));
        }

        let active_turn = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?;
        if options.interrupt_before_delivery
            && !options.resuming
            && request.delivery_policy == "interrupt_now"
            && active_turn.is_some()
        {
            if !session.capabilities.interrupt {
                self.mark_failed(
                    message_id,
                    "session ".to_string() + session_id + " runtime does not support interrupt",
                )
                .await?;
            } else if let Err(error) = self
                .turns
                .interrupt_current_turn_locked(session_id, actor)
                .await
            {
                if matches!(error, Error::ControlUnknown(_)) {
                    self.mark_unknown(message_id, &error.to_string()).await?;
                } else {
                    self.mark_failed(message_id, error.to_string()).await?;
                }
            }
        }

        if !options.resuming && options.drain_now {
            self.drain_inbox_locked(session_id, actor).await?;
        }

        let message = self
            .get_message(session_id, message_id)
            .await?
            .ok_or_else(|| Error::Domain("submitted inbox message missing".to_string()))?;
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

        let rows_affected = SqliteInboxRepository::new(self.pool.clone())
            .cancel_pending_message(session_id, message_id)
            .await?;

        let message = self
            .get_message(session_id, message_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("inbox message {message_id} not found")))?;
        if rows_affected == 0 && message.state != "cancelled" {
            return Err(Error::StateConflict(format!(
                "inbox message {message_id} is not pending"
            )));
        }
        if rows_affected > 0 {
            self.notify_available(session_id);
        }
        let message = self.get_message(session_id, message_id).await?.unwrap();
        Ok(InboxCommandOutcome {
            data: json!({ "inbox_message": message }),
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

        let rows_affected = SqliteInboxRepository::new(self.pool.clone())
            .dismiss_failed_message(session_id, message_id)
            .await?;

        let message = self
            .get_message(session_id, message_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("inbox message {message_id} not found")))?;
        if rows_affected == 0 && message.state != "dismissed" {
            return Err(Error::StateConflict(format!(
                "inbox message {message_id} is not failed"
            )));
        }
        if rows_affected > 0 {
            self.notify_available(session_id);
        }
        let message = self.get_message(session_id, message_id).await?.unwrap();
        Ok(InboxCommandOutcome {
            data: json!({ "inbox_message": message }),
            duplicate: false,
        })
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
        let Some(row) = inbox_repository.next_pending_message(session_id).await? else {
            return Ok(());
        };
        if let Some(expected) = row.required_runtime_id.as_deref()
            && let Err(error) = crate::runtime::ControlTarget::resolve(
                &self.pool,
                self.clients.registry(),
                session_id,
                Some(expected),
            )
            .await
        {
            self.mark_failed(&row.message_id, error.to_string()).await?;
            return Ok(());
        }
        if !adapter.input_available(session_id).await? {
            return Ok(());
        }
        if matches!(session.state.as_str(), "exited" | "error") {
            return Ok(());
        }
        let prepared_turn_id = row.turn_id.clone();
        let steer_turn_id = if row.delivery_policy == "steer" {
            if active.as_ref().map(|turn| &turn.turn_id) != row.steer_target_turn_id.as_ref() {
                self.mark_failed(
                    &row.message_id,
                    "Steer target Turn changed; input was not submitted".into(),
                )
                .await?;
                self.notify_available(session_id);
                return Ok(());
            }
            row.steer_target_turn_id
        } else {
            if active.as_ref().map(|turn| &turn.turn_id) != prepared_turn_id.as_ref()
                && active.is_some()
            {
                return Ok(());
            }
            None
        };
        if steer_turn_id.is_none() && actor.phase() != scheduling::SessionPhase::Idle {
            return Ok(());
        }
        if steer_turn_id.is_none()
            && !matches!(session.state.as_str(), "idle" | "interrupted")
            && !(matches!(session.state.as_str(), "created" | "starting")
                && adapter.prepares_on_input())
        {
            return Ok(());
        }
        let delivery_policy = row.delivery_policy;
        let message_id = row.message_id;
        let input = row.input_summary;
        let metadata = row.metadata;
        let mut metadata: Value = serde_json::from_str(&metadata)?;
        if !metadata.is_object() {
            metadata = json!({});
        }
        let rows_affected = inbox_repository.mark_dispatching(&message_id).await?;
        if rows_affected == 0 {
            return Ok(());
        }

        let turns = &self.turns;
        let delivery = match steer_turn_id {
            Some(turn_id) => {
                turns
                    .submit_steer(
                        session_id,
                        input.into(),
                        &turn_id,
                        row.required_runtime_id.as_deref(),
                    )
                    .await
            }
            None => {
                actor.set_phase(scheduling::SessionPhase::StartingTurn);
                turns
                    .submit_start_turn(
                        session_id,
                        input.into(),
                        metadata,
                        row.required_runtime_id.as_deref(),
                        prepared_turn_id.as_deref(),
                    )
                    .await
            }
        };
        let delivery = match delivery {
            Ok((_, crate::control::ControlResult::Unsupported(_)))
                if delivery_policy == "steer" =>
            {
                inbox_repository.wait_after_idle(&message_id).await?;
                self.notify_available(session_id);
                return Ok(());
            }
            Ok((turn_id, result)) => result
                .into_delivery_result()
                .map(|receipt| (turn_id, receipt)),
            Err(error) => Err(crate::DeliveryError::from(error)),
        };
        match delivery {
            Ok((turn_id, _receipt)) => {
                actor.finish_start();
                inbox_repository
                    .mark_dispatched(&message_id, turn_id.as_deref())
                    .await?;
                self.notify_available(session_id);
            }
            Err(crate::DeliveryError::Offline(_)) => {
                actor.cancel_start();
                if let Some(turn_id) = prepared_turn_id.as_deref() {
                    inbox_repository
                        .detach_prepared_turn(&message_id, turn_id)
                        .await?;
                }
                inbox_repository.return_pending(&message_id).await?;
            }
            Err(crate::DeliveryError::StateChanged(_)) if delivery_policy != "steer" => {
                actor.cancel_start();
                if let Some(turn_id) = prepared_turn_id.as_deref() {
                    inbox_repository
                        .detach_prepared_turn(&message_id, turn_id)
                        .await?;
                }
                inbox_repository.return_pending(&message_id).await?;
            }
            Err(crate::DeliveryError::Uncertain(reason)) => {
                self.mark_unknown(&message_id, &reason).await?;
            }
            Err(error) => {
                actor.cancel_start();
                self.mark_failed(&message_id, error.to_string()).await?;
                self.notify_available(session_id);
            }
        }
        Ok(())
    }

    async fn mark_failed(&self, message_id: &str, failure_message: String) -> Result<()> {
        let changed = SqliteInboxRepository::new(self.pool.clone())
            .mark_failed(message_id, &failure_message)
            .await?;
        if changed > 0 {
            let session_id: String =
                sqlx::query_scalar("SELECT session_id FROM inbox_messages WHERE message_id=?")
                    .bind(message_id)
                    .fetch_one(&self.pool)
                    .await?;
            self.notify_available(&session_id);
        }
        Ok(())
    }

    async fn mark_unknown(&self, message_id: &str, reason: &str) -> Result<()> {
        SqliteInboxRepository::new(self.pool.clone())
            .mark_unknown(message_id, reason)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
