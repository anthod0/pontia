use std::str::FromStr;

use sqlx::{Row, SqlitePool};

use crate::{
    config::AppConfig,
    domain::{
        DomainEvent, EventSource, EventType, ProjectionState, SessionProjection, SessionState,
        TurnProjection, TurnState,
    },
    error::Result,
    storage::sqlite::{connect_sqlite, run_migrations},
};

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
}

pub async fn initialize(config: &AppConfig) -> Result<AppState> {
    let db = connect_sqlite(&config.database_url).await?;

    if config.run_migrations {
        run_migrations(&db).await?;
    }

    Ok(AppState { db })
}

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
               FROM events WHERE session_id = ? ORDER BY created_at, event_id"#,
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

fn row_to_session(row: sqlx::sqlite::SqliteRow) -> Result<SessionProjection> {
    let metadata: String = row.try_get("metadata")?;
    let state: String = row.try_get("state")?;

    Ok(SessionProjection {
        session_id: row.try_get("session_id")?,
        client_type: row.try_get("client_type")?,
        state: SessionState::from_str(&state)?,
        current_turn_id: row.try_get("current_turn_id")?,
        state_version: row.try_get("state_version")?,
        metadata: serde_json::from_str(&metadata)?,
    })
}

fn row_to_turn(row: sqlx::sqlite::SqliteRow) -> Result<TurnProjection> {
    let metadata: String = row.try_get("metadata")?;
    let state: String = row.try_get("state")?;

    Ok(TurnProjection {
        turn_id: row.try_get("turn_id")?,
        session_id: row.try_get("session_id")?,
        state: TurnState::from_str(&state)?,
        state_version: row.try_get("state_version")?,
        metadata: serde_json::from_str(&metadata)?,
    })
}

fn row_to_event(row: sqlx::sqlite::SqliteRow) -> Result<DomainEvent> {
    let payload: String = row.try_get("payload")?;
    let source: String = row.try_get("source")?;
    let event_type: String = row.try_get("event_type")?;
    let occurred_at: String = row.try_get("occurred_at")?;

    Ok(DomainEvent {
        event_id: row.try_get("event_id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        source: EventSource::from_str(&source)?,
        client_type: row.try_get("client_type")?,
        event_type: EventType::from_str(&event_type)?,
        occurred_at: time::OffsetDateTime::parse(
            &occurred_at,
            &time::format_description::well_known::Rfc3339,
        )
        .map_err(|err| crate::error::Error::Domain(format!("invalid event timestamp: {err}")))?,
        seq: row.try_get("seq")?,
        payload: serde_json::from_str(&payload)?,
    })
}
