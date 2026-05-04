use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventIngestResult {
    pub accepted: bool,
    pub duplicate: bool,
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub state_version: i64,
}

#[derive(Clone)]
pub struct EventIngestService {
    pool: SqlitePool,
}

impl EventIngestService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn ingest_event(&self, event: DomainEvent) -> Result<EventIngestResult> {
        if let Some(existing_version) = self
            .existing_event_state_version(&event.event_id, &event.session_id)
            .await?
        {
            return Ok(EventIngestResult {
                accepted: true,
                duplicate: true,
                event_id: event.event_id,
                session_id: event.session_id,
                turn_id: event.turn_id,
                state_version: existing_version,
            });
        }

        let sessions = self.load_session_projection(&event.session_id).await?;
        let turns = self.load_turn_projections(&event.session_id).await?;
        let mut projection = ProjectionState::with_existing(sessions, turns);
        projection.apply(&event)?;

        let mut tx = self.pool.begin().await?;
        let payload = serde_json::to_string(&event.payload)?;
        let occurred_at = event
            .occurred_at
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|err| {
                crate::error::Error::Domain(format!("invalid event timestamp: {err}"))
            })?;

        sqlx::query(
            r#"INSERT INTO events
               (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, seq, payload)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&event.event_id)
        .bind(&event.session_id)
        .bind(&event.turn_id)
        .bind(event.source.to_string())
        .bind(&event.client_type)
        .bind(event.event_type.to_string())
        .bind(occurred_at)
        .bind(event.seq)
        .bind(payload)
        .execute(&mut *tx)
        .await?;

        let state_version: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id = ?")
                .bind(&event.session_id)
                .fetch_one(&mut *tx)
                .await?;

        for session in projection.sessions() {
            let metadata = serde_json::to_string(&session.metadata)?;
            sqlx::query(
                r#"INSERT INTO sessions
                   (session_id, client_type, state, current_turn_id, state_version, metadata)
                   VALUES (?, ?, ?, ?, ?, ?)
                   ON CONFLICT(session_id) DO UPDATE SET
                       client_type = excluded.client_type,
                       state = excluded.state,
                       current_turn_id = excluded.current_turn_id,
                       state_version = excluded.state_version,
                       metadata = excluded.metadata,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
            )
            .bind(&session.session_id)
            .bind(&session.client_type)
            .bind(session.state.to_string())
            .bind(&session.current_turn_id)
            .bind(state_version)
            .bind(metadata)
            .execute(&mut *tx)
            .await?;
        }

        for turn in projection.turns() {
            let metadata = serde_json::to_string(&turn.metadata)?;
            sqlx::query(
                r#"INSERT INTO turns
                   (turn_id, session_id, state, state_version, metadata)
                   VALUES (?, ?, ?, ?, ?)
                   ON CONFLICT(turn_id) DO UPDATE SET
                       session_id = excluded.session_id,
                       state = excluded.state,
                       state_version = excluded.state_version,
                       metadata = excluded.metadata,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
            )
            .bind(&turn.turn_id)
            .bind(&turn.session_id)
            .bind(turn.state.to_string())
            .bind(turn.state_version)
            .bind(metadata)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        TaskCommandService::new(self.pool.clone())
            .sync_task_from_turn_event(&event)
            .await?;

        Ok(EventIngestResult {
            accepted: true,
            duplicate: false,
            event_id: event.event_id,
            session_id: event.session_id,
            turn_id: event.turn_id,
            state_version,
        })
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionProjection>> {
        let mut sessions = self.load_session_projection(session_id).await?;
        Ok(sessions.pop())
    }

    pub async fn get_turn(&self, turn_id: &str) -> Result<Option<TurnProjection>> {
        let row = sqlx::query(
            "SELECT turn_id, session_id, state, state_version, metadata FROM turns WHERE turn_id = ?",
        )
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_turn).transpose()
    }

    pub async fn list_events(&self, session_id: &str) -> Result<Vec<DomainEvent>> {
        let rows = sqlx::query(
            r#"SELECT event_id, session_id, turn_id, source, client_type, event_type, occurred_at, seq, payload
               FROM events WHERE session_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_event).collect()
    }

    pub async fn sequence_warnings(&self, event: &DomainEvent) -> Result<Vec<String>> {
        let Some(seq) = event.seq else {
            return Ok(Vec::new());
        };

        let max_seq: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(seq) FROM events WHERE session_id = ? AND seq IS NOT NULL",
        )
        .bind(&event.session_id)
        .fetch_one(&self.pool)
        .await?;

        let Some(max_seq) = max_seq else {
            return Ok(Vec::new());
        };

        let warning = if seq <= max_seq {
            Some(format!(
                "non-monotonic sequence: received seq {seq} after max seq {max_seq}"
            ))
        } else if seq > max_seq + 1 {
            Some(format!(
                "sequence gap: received seq {seq} after max seq {max_seq}"
            ))
        } else {
            None
        };

        Ok(warning.into_iter().collect())
    }

    pub async fn record_warnings(&self, event: &DomainEvent, warnings: &[String]) -> Result<()> {
        for warning in warnings {
            sqlx::query(
                "INSERT INTO ingest_warnings (event_id, session_id, warning) VALUES (?, ?, ?)",
            )
            .bind(&event.event_id)
            .bind(&event.session_id)
            .bind(warning)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn existing_event_state_version(
        &self,
        event_id: &str,
        session_id: &str,
    ) -> Result<Option<i64>> {
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM events WHERE event_id = ?")
            .bind(event_id)
            .fetch_optional(&self.pool)
            .await?;

        if exists.is_none() {
            return Ok(None);
        }

        let version = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(Some(version))
    }

    async fn load_session_projection(&self, session_id: &str) -> Result<Vec<SessionProjection>> {
        let rows = sqlx::query(
            "SELECT session_id, client_type, state, current_turn_id, state_version, metadata FROM sessions WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_session).collect()
    }

    async fn load_turn_projections(&self, session_id: &str) -> Result<Vec<TurnProjection>> {
        let rows = sqlx::query(
            "SELECT turn_id, session_id, state, state_version, metadata FROM turns WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_turn).collect()
    }
}

pub(crate) fn nested_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(ToString::to_string)
}

pub(crate) fn nested_array_strings(value: &Value, path: &[&str]) -> Option<Vec<String>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(
        current
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
    )
}

pub(crate) fn remove_internal_metadata_fields(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("source_ref");
    }
}
