mod preparation;
mod retry;
pub use retry::RetryInboxMessageRequest;
mod association;
pub(crate) use association::InboxAssociations;
mod scheduling;
pub(crate) use scheduling::InboxScheduler;

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
    BranchReplayService, ExternalQueryService, InboxMessageView, PontiaEvent, PontiaEventSource,
    PontiaEventType, TurnCommandService, views::inbox::row_to_view,
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SubmitInboxMessageRequest {
    pub input: String,
    #[serde(default = "default_delivery_policy")]
    pub delivery_policy: String,
    #[serde(default)]
    pub branch_target_turn_id: Option<String>,
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
    event_ingest: crate::EventIngestService,
    queries: ExternalQueryService,
    clients: crate::clients::ClientExecutionService,
    turns: TurnCommandService,
    branches: BranchReplayService,
    scheduler: InboxScheduler,
}

impl InboxCommandService {
    pub(crate) fn new(
        pool: SqlitePool,
        event_ingest: crate::EventIngestService,
        queries: ExternalQueryService,
        clients: crate::clients::ClientExecutionService,
        turns: TurnCommandService,
        branches: BranchReplayService,
        scheduler: InboxScheduler,
    ) -> Self {
        Self {
            pool,
            event_ingest,
            queries,
            clients,
            turns,
            branches,
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
    /// identity for the same Session does not enqueue a second message.
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
        let lock = self.scheduler.command_lock(session_id);
        let _guard = lock.lock().await;
        self.enqueue_message(message_id, session_id, request, None, false)
            .await
    }

    async fn enqueue_message(
        &self,
        message_id: &str,
        session_id: &str,
        request: SubmitInboxMessageRequest,
        retry_of: Option<&str>,
        resuming: bool,
    ) -> Result<InboxCommandOutcome> {
        if message_id.is_empty() || message_id.len() > 512 {
            return Err(Error::Domain("Invalid Inbox submission identity".into()));
        }
        let inbox_repository = SqliteInboxRepository::new(self.pool.clone());
        let payload = serde_json::to_string(&request)?;
        if let Some(existing) = inbox_repository.get_message(session_id, message_id).await? {
            if existing.submission_payload.as_deref() != Some(&payload)
                || existing.retry_of_message_id.as_deref() != retry_of
            {
                return Err(Error::StateConflict(
                    "Submission identity was already used for different input".into(),
                ));
            }
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
        if request.branch_target_turn_id.is_some() {
            if request.delivery_policy != "after_idle" {
                return Err(Error::Domain(
                    "branch submissions require after_idle delivery".to_string(),
                ));
            }
            if !session.capabilities.branch_control {
                return Err(Error::CapabilityUnavailable(format!(
                    "session {session_id} does not support branch control"
                )));
            }
            self.branches
                .validate_submission(
                    session_id,
                    request
                        .branch_target_turn_id
                        .as_deref()
                        .expect("branch target was checked above"),
                )
                .await?;
        }
        if request.delivery_policy == "steer"
            && !self
                .clients
                .for_client(&session.client_type)?
                .supports_steer()
        {
            return Err(Error::CapabilityUnavailable(
                "This client does not support steer".into(),
            ));
        }

        let metadata = serde_json::to_string(&request.metadata)?;

        let active_turn = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?;
        let steer_target = if request.delivery_policy == "steer" {
            active_turn.as_ref().map(|turn| turn.turn_id.as_str())
        } else {
            None
        };
        let inserted = inbox_repository
            .enqueue(
                pontia_storage_sqlite::repositories::inbox::NewInboxMessage {
                    message_id,
                    session_id,
                    delivery_policy: &request.delivery_policy,
                    input: &request.input,
                    metadata: &metadata,
                    branch_target: request.branch_target_turn_id.as_deref(),
                    steer_target,
                    submission_payload: &payload,
                    retry_of,
                    resuming,
                },
            )
            .await?;
        if !inserted {
            return Err(Error::StateConflict(
                "Submission identity belongs to another Session".into(),
            ));
        }

        self.audit(
            session_id,
            &session.client_type,
            PontiaEventType::InboxMessageQueued,
            json!({ "message_id": message_id, "delivery_policy": request.delivery_policy }),
        )
        .await?;

        let active_turn = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?;
        if !resuming && request.delivery_policy == "interrupt_now" && active_turn.is_some() {
            if !session.capabilities.interrupt {
                self.mark_failed(
                    message_id,
                    "session ".to_string() + session_id + " runtime does not support interrupt",
                )
                .await?;
            } else if let Err(error) = self.turns.interrupt_current_turn(session_id).await {
                if matches!(error, Error::ControlUnknown(_)) {
                    self.mark_unknown(message_id, &error.to_string()).await?;
                } else {
                    self.mark_failed(message_id, error.to_string()).await?;
                }
            }
        }

        if !resuming {
            self.drain_inbox(session_id).await?;
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
        let session = query
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
            self.audit(
                session_id,
                &session.client_type,
                PontiaEventType::InboxMessageCancelled,
                json!({ "message_id": message_id }),
            )
            .await?;
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
        let session = query
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
            self.audit(
                session_id,
                &session.client_type,
                PontiaEventType::InboxMessageDismissed,
                json!({ "message_id": message_id }),
            )
            .await?;
        }
        let message = self.get_message(session_id, message_id).await?.unwrap();
        Ok(InboxCommandOutcome {
            data: json!({ "inbox_message": message }),
            duplicate: false,
        })
    }

    pub async fn drain_inbox(&self, session_id: &str) -> Result<()> {
        let lock = self.scheduler.session_lock(session_id);
        let _guard = lock.lock().await;
        if self.scheduler.awaiting_initial(session_id) {
            return Ok(());
        }
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
        if let Some(expected) = row.required_runtime_instance_id.as_deref()
            && let Err(error) =
                crate::runtime::ControlTarget::resolve(&self.pool, session_id, Some(expected)).await
        {
            self.mark_failed(&row.message_id, error.to_string()).await?;
            return Ok(());
        }
        if row.branch_target_turn_id.is_none() && !adapter.input_available(session_id).await? {
            return Ok(());
        }
        if matches!(session.state.as_str(), "exited" | "error") {
            return Ok(());
        }
        let intent = if row.delivery_policy == "steer" {
            if active.as_ref().map(|turn| &turn.turn_id) != row.steer_target_turn_id.as_ref() {
                self.mark_failed(
                    &row.message_id,
                    "Steer target Turn changed; input was not submitted".into(),
                )
                .await?;
                self.notify_available(session_id);
                return Ok(());
            }
            match row.steer_target_turn_id {
                Some(turn_id) => crate::turns::InputIntent::Steer { turn_id },
                None => crate::turns::InputIntent::Start,
            }
        } else {
            if active.is_some() {
                return Ok(());
            }
            crate::turns::InputIntent::Start
        };
        if matches!(intent, crate::turns::InputIntent::Start)
            && !matches!(session.state.as_str(), "idle" | "interrupted")
            && !(matches!(session.state.as_str(), "created" | "starting")
                && adapter.prepares_on_input())
        {
            return Ok(());
        }
        let delivery_policy = row.delivery_policy;
        let message_id = row.message_id;
        let input = row.input_summary;
        let branch_target_turn_id = row.branch_target_turn_id;
        let metadata = row.metadata;
        let mut metadata: Value = serde_json::from_str(&metadata)?;
        if !metadata.is_object() {
            metadata = json!({});
        }
        if let Value::Object(ref mut object) = metadata {
            object.insert(
                "inbox_message_id".to_string(),
                Value::String(message_id.clone()),
            );
        }
        let rows_affected = inbox_repository.mark_dispatching(&message_id).await?;
        if rows_affected == 0 {
            return Ok(());
        }

        let turns = &self.turns;
        let delivery = if branch_target_turn_id.is_some() {
            self.branches
                .dispatch(&self.clients, session_id, &message_id)
                .await
                .map(|result| (None, result))
        } else {
            turns
                .submit_input(
                    session_id,
                    input,
                    metadata,
                    intent,
                    row.required_runtime_instance_id.as_deref(),
                )
                .await
        };
        let delivery = match delivery {
            Ok((turn, result)) => result.into_result().map(|receipt| (turn, receipt)),
            Err(error) => Err(error),
        };
        match delivery {
            Ok((turn, receipt)) => {
                InboxAssociations::new(self.pool.clone())
                    .record_receipt(session_id, &message_id, &receipt)
                    .await?;
                let turn_id = turn.as_ref().map(|turn| turn.turn_id.as_str());
                inbox_repository
                    .mark_dispatched(&message_id, turn_id)
                    .await?;
                let mut payload = json!({ "message_id": message_id });
                if let Some(turn) = turn {
                    payload["turn_id"] = json!(turn.turn_id);
                }
                self.audit(
                    session_id,
                    &session.client_type,
                    PontiaEventType::InboxMessageDispatched,
                    payload,
                )
                .await?;
                self.notify_available(session_id);
            }
            Err(Error::Conflict {
                code: "input_busy", ..
            }) if delivery_policy != "steer" => {
                inbox_repository.return_pending(&message_id).await?;
            }
            Err(Error::ControlUnknown(reason)) => {
                self.mark_unknown(&message_id, &reason).await?;
            }
            Err(error) => {
                self.mark_failed(&message_id, error.to_string()).await?;
                self.notify_available(session_id);
            }
        }
        Ok(())
    }

    async fn mark_failed(&self, message_id: &str, failure_message: String) -> Result<()> {
        if SqliteInboxRepository::new(self.pool.clone())
            .mark_failed(message_id, &failure_message)
            .await?
            == 0
        {
            return Ok(());
        }
        self.audit_delivery(message_id, PontiaEventType::InboxMessageFailed)
            .await
    }

    async fn mark_unknown(&self, message_id: &str, reason: &str) -> Result<()> {
        if SqliteInboxRepository::new(self.pool.clone())
            .mark_unknown(message_id, reason)
            .await?
            == 0
        {
            return Ok(());
        }
        self.audit_delivery(message_id, PontiaEventType::InboxMessageDeliveryUnknown)
            .await
    }

    async fn audit_delivery(&self, message_id: &str, event_type: PontiaEventType) -> Result<()> {
        let (session_id, client_type, state): (String, String, String) = sqlx::query_as("SELECT m.session_id,s.client_type,m.state FROM inbox_messages m JOIN sessions s ON s.session_id=m.session_id WHERE m.message_id=?")
            .bind(message_id).fetch_one(&self.pool).await?;
        let event_type = if state == "dispatched" {
            PontiaEventType::InboxMessageDispatched
        } else {
            event_type
        };
        self.audit(
            &session_id,
            &client_type,
            event_type,
            json!({"message_id":message_id,"state":state}),
        )
        .await
    }

    async fn audit(
        &self,
        session_id: &str,
        client_type: &str,
        event_type: PontiaEventType,
        payload: Value,
    ) -> Result<()> {
        self.event_ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id,
                None,
                PontiaEventSource::ExternalApi,
                client_type,
                event_type,
                payload,
            ))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
