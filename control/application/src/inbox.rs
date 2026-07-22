use super::*;
use pontia_storage_sqlite::repositories::{
    inbox::SqliteInboxRepository, turns::SqliteTurnRepository,
};

#[derive(Debug, Clone, Deserialize, PartialEq)]
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
}

impl InboxCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn submit_message(
        &self,
        session_id: &str,
        request: SubmitInboxMessageRequest,
    ) -> Result<InboxCommandOutcome> {
        if !matches!(
            request.delivery_policy.as_str(),
            "after_idle" | "interrupt_now"
        ) {
            return Err(Error::Domain(format!(
                "unknown delivery_policy: {}",
                request.delivery_policy
            )));
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        let message_id = new_message_id().to_string();
        let metadata = serde_json::to_string(&request.metadata)?;

        let inbox_repository = SqliteInboxRepository::new(self.pool.clone());
        if request.delivery_policy == "interrupt_now" {
            inbox_repository
                .supersede_pending_interrupts(session_id, &message_id)
                .await?;
        }
        inbox_repository
            .insert_message(
                &message_id,
                session_id,
                &request.delivery_policy,
                &request.input,
                &metadata,
            )
            .await?;

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
        if request.delivery_policy == "interrupt_now" && active_turn.is_some() {
            if !session.capabilities.interrupt {
                self.mark_failed(
                    &message_id,
                    "session ".to_string() + session_id + " runtime does not support interrupt",
                )
                .await?;
            } else if let Err(error) = RuntimeControlService::new(self.pool.clone())
                .interrupt_current_turn(session_id)
                .await
            {
                self.mark_failed(&message_id, error.to_string()).await?;
            }
        }

        self.drain_inbox(session_id).await?;

        let message = self
            .get_message(session_id, &message_id)
            .await?
            .ok_or_else(|| Error::Domain("submitted inbox message missing".to_string()))?;
        Ok(InboxCommandOutcome {
            data: json!({ "inbox_message": message }),
            duplicate: false,
        })
    }

    pub async fn list_messages(&self, session_id: &str) -> Result<Vec<InboxMessageView>> {
        ExternalQueryService::new(self.pool.clone())
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let rows = SqliteInboxRepository::new(self.pool.clone())
            .list_messages(session_id)
            .await?;
        rows.into_iter().map(row_to_inbox_message_view).collect()
    }

    pub async fn get_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<Option<InboxMessageView>> {
        let row = SqliteInboxRepository::new(self.pool.clone())
            .get_message(session_id, message_id)
            .await?;
        row.map(row_to_inbox_message_view).transpose()
    }

    pub async fn cancel_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<InboxCommandOutcome> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
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
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
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
        let query = ExternalQueryService::new(self.pool.clone());
        let session = match query.get_session(session_id).await? {
            Some(session) => session,
            None => return Ok(()),
        };
        if !matches!(session.state.as_str(), "idle" | "interrupted")
            || SqliteTurnRepository::new(self.pool.clone())
                .active_turn(session_id)
                .await?
                .is_some()
        {
            return Ok(());
        }

        let inbox_repository = SqliteInboxRepository::new(self.pool.clone());
        let Some(row) = inbox_repository.next_pending_message(session_id).await? else {
            return Ok(());
        };
        let message_id = row.message_id;
        let input = row.input_summary;
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

        match TurnCommandService::new(self.pool.clone())
            .create_and_dispatch_turn(session_id, input, metadata)
            .await
        {
            Ok(turn) => {
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
            }
            Err(error) => {
                self.mark_failed(&message_id, error.to_string()).await?;
            }
        }
        Ok(())
    }

    async fn mark_failed(&self, message_id: &str, failure_message: String) -> Result<()> {
        SqliteInboxRepository::new(self.pool.clone())
            .mark_failed(message_id, &failure_message)
            .await
    }

    async fn audit(
        &self,
        session_id: &str,
        client_type: &str,
        event_type: PontiaEventType,
        payload: Value,
    ) -> Result<()> {
        EventIngestService::new(self.pool.clone())
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
