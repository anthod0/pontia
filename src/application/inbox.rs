use super::*;

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

        if request.delivery_policy == "interrupt_now" {
            let mut tx = self.pool.begin().await?;
            sqlx::query(
                r#"UPDATE inbox_messages
                   SET state = 'superseded', superseded_by_message_id = ?,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE session_id = ? AND delivery_policy = 'interrupt_now' AND state = 'pending'"#,
            )
            .bind(&message_id)
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                r#"INSERT INTO inbox_messages
                   (message_id, session_id, state, delivery_policy, input_summary, metadata)
                   VALUES (?, ?, 'pending', ?, ?, ?)"#,
            )
            .bind(&message_id)
            .bind(session_id)
            .bind(&request.delivery_policy)
            .bind(&request.input)
            .bind(&metadata)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        } else {
            sqlx::query(
                r#"INSERT INTO inbox_messages
                   (message_id, session_id, state, delivery_policy, input_summary, metadata)
                   VALUES (?, ?, 'pending', ?, ?, ?)"#,
            )
            .bind(&message_id)
            .bind(session_id)
            .bind(&request.delivery_policy)
            .bind(&request.input)
            .bind(&metadata)
            .execute(&self.pool)
            .await?;
        }

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
        let rows = sqlx::query(
            r#"SELECT message_id, session_id, state, delivery_policy, input_summary, metadata,
                      turn_id, superseded_by_message_id, failure_message, created_at, updated_at,
                      dispatched_at, cancelled_at
               FROM inbox_messages WHERE session_id = ? ORDER BY created_at, message_id"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_inbox_message_view).collect()
    }

    pub async fn get_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<Option<InboxMessageView>> {
        let row = sqlx::query(
            r#"SELECT message_id, session_id, state, delivery_policy, input_summary, metadata,
                      turn_id, superseded_by_message_id, failure_message, created_at, updated_at,
                      dispatched_at, cancelled_at
               FROM inbox_messages WHERE session_id = ? AND message_id = ?"#,
        )
        .bind(session_id)
        .bind(message_id)
        .fetch_optional(&self.pool)
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

        let result = sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'cancelled', cancelled_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE session_id = ? AND message_id = ? AND state = 'pending'"#,
        )
        .bind(session_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?;

        let message = self
            .get_message(session_id, message_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("inbox message {message_id} not found")))?;
        if result.rows_affected() == 0 && message.state != "cancelled" {
            return Err(Error::StateConflict(format!(
                "inbox message {message_id} is not pending"
            )));
        }
        if result.rows_affected() > 0 {
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

        let row = sqlx::query(
            r#"SELECT message_id, input_summary, metadata
               FROM inbox_messages
               WHERE session_id = ? AND state = 'pending'
               ORDER BY CASE WHEN delivery_policy = 'interrupt_now' THEN 0 ELSE 1 END,
                        CASE WHEN delivery_policy = 'interrupt_now' THEN created_at END DESC,
                        CASE WHEN delivery_policy = 'interrupt_now' THEN message_id END DESC,
                        created_at ASC,
                        message_id ASC
               LIMIT 1"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(());
        };
        let message_id: String = row.try_get("message_id")?;
        let input: String = row.try_get("input_summary")?;
        let metadata: String = row.try_get("metadata")?;
        let mut metadata: Value = serde_json::from_str(&metadata)?;
        if let Value::Object(ref mut object) = metadata {
            object.insert(
                "inbox_message_id".to_string(),
                Value::String(message_id.clone()),
            );
        }
        inherit_dag_planning_context(&mut metadata, &session.metadata);

        let result = sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'dispatching', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE message_id = ? AND state = 'pending'"#,
        )
        .bind(&message_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Ok(());
        }

        match TurnCommandService::new(self.pool.clone())
            .create_and_dispatch_turn(session_id, input, metadata)
            .await
        {
            Ok(turn) => {
                sqlx::query(
                    r#"UPDATE inbox_messages
                       SET state = 'dispatched', turn_id = ?, dispatched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                           updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE message_id = ?"#,
                )
                .bind(&turn.turn_id)
                .bind(&message_id)
                .execute(&self.pool)
                .await?;
                self.audit(
                    session_id,
                    &session.client_type,
                    EventType::InboxMessageDispatched,
                    json!({ "message_id": message_id, "turn_id": turn.turn_id }),
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
        sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'failed', failure_message = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE message_id = ? AND state IN ('pending', 'dispatching')"#,
        )
        .bind(failure_message)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn audit(
        &self,
        session_id: &str,
        client_type: &str,
        event_type: EventType,
        payload: Value,
    ) -> Result<()> {
        EventIngestService::new(self.pool.clone())
            .ingest_event(DomainEvent::new(
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
        let response: Option<String> = sqlx::query_scalar(
            "SELECT response FROM idempotency_keys WHERE operation = ? AND key = ?",
        )
        .bind(operation)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        response
            .map(|value| {
                let value: Value = serde_json::from_str(&value)?;
                Ok(value["message_id"].as_str().map(ToString::to_string))
            })
            .transpose()
            .map(|value| value.flatten())
    }

    async fn store_idempotency_message_id(
        &self,
        operation: &str,
        key: &str,
        message_id: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO idempotency_keys (operation, key, response)
               VALUES (?, ?, ?)
               ON CONFLICT(operation, key) DO NOTHING"#,
        )
        .bind(operation)
        .bind(key)
        .bind(json!({ "message_id": message_id }).to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
