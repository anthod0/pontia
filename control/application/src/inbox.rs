use super::*;
use pontia_storage_sqlite::repositories::{
    idempotency::SqliteIdempotencyRepository, inbox::SqliteInboxRepository,
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

fn inherit_dag_planning_context(turn_metadata: &mut Value, session_metadata: &Value) {
    if session_metadata.get("dag_managed").and_then(Value::as_bool) != Some(true)
        || session_metadata
            .get("dag_planning_role")
            .and_then(Value::as_str)
            .is_none()
    {
        return;
    }

    if !turn_metadata.is_object() {
        *turn_metadata = json!({});
    }
    if let Some(turn_object) = turn_metadata.as_object_mut() {
        copy_dag_planning_context(turn_object, session_metadata);
    }
}

fn copy_dag_planning_context(
    turn_object: &mut serde_json::Map<String, Value>,
    session_metadata: &Value,
) {
    for key in ["dag_managed", "dag_planning_role", "task_id", "planning"] {
        if let Some(value) = session_metadata.get(key) {
            turn_object.insert(key.to_string(), value.clone());
        }
    }
}

impl InboxCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn submit_message(
        &self,
        session_id: &str,
        request: SubmitInboxMessageRequest,
        idempotency_key: Option<&str>,
    ) -> Result<InboxCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(message_id) = self
                .idempotency_message_id(&format!("submit_inbox_message:{session_id}"), key)
                .await?
        {
            let message = self
                .get_message(session_id, &message_id)
                .await?
                .ok_or_else(|| {
                    Error::Domain(format!("idempotent inbox message {message_id} missing"))
                })?;
            return Ok(InboxCommandOutcome {
                data: json!({ "inbox_message": message }),
                duplicate: true,
            });
        }

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
            EventType::InboxMessageQueued,
            json!({ "message_id": message_id, "delivery_policy": request.delivery_policy }),
        )
        .await?;

        if request.delivery_policy == "interrupt_now" && session.current_turn_id.is_some() {
            if !session.capabilities.interrupt {
                self.mark_failed(
                    &message_id,
                    "session ".to_string() + session_id + " runtime does not support interrupt",
                )
                .await?;
            } else if let Err(error) = RuntimeControlService::new(self.pool.clone())
                .interrupt_current_turn(session_id, None)
                .await
            {
                self.mark_failed(&message_id, error.to_string()).await?;
            }
        }

        self.drain_inbox(session_id).await?;

        if let Some(key) = idempotency_key {
            self.store_idempotency_message_id(
                &format!("submit_inbox_message:{session_id}"),
                key,
                &message_id,
            )
            .await?;
        }

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
                EventType::InboxMessageCancelled,
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
                EventType::InboxMessageDismissed,
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
            || session.current_turn_id.is_some()
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
        inherit_dag_planning_context(&mut metadata, &session.metadata);

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
                    EventType::InboxMessageDispatched,
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
        event_type: EventType,
        payload: Value,
    ) -> Result<()> {
        EventIngestService::new(self.pool.clone())
            .ingest_event(ReportedEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::ExternalApi,
                client_type.to_string(),
                event_type,
                payload,
            ))
            .await?;
        Ok(())
    }

    async fn idempotency_message_id(&self, operation: &str, key: &str) -> Result<Option<String>> {
        Ok(SqliteIdempotencyRepository::new(self.pool.clone())
            .get_response(operation, key)
            .await?
            .and_then(|value| value["message_id"].as_str().map(ToString::to_string)))
    }

    async fn store_idempotency_message_id(
        &self,
        operation: &str,
        key: &str,
        message_id: &str,
    ) -> Result<()> {
        SqliteIdempotencyRepository::new(self.pool.clone())
            .store_response(operation, key, &json!({ "message_id": message_id }))
            .await
    }
}
