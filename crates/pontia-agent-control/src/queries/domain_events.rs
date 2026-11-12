use pontia_core::Result;
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, FromRow)]
pub struct DomainEventRecord {
    pub position: i64,
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    pub event_type: String,
    pub occurred_at: String,
    pub payload: String,
    pub created_at: String,
    pub application_id: Option<String>,
}

#[derive(Debug)]
pub struct DomainEventPage {
    pub events: Vec<DomainEventRecord>,
    pub next_position: i64,
    pub head_position: i64,
}

#[derive(Debug, Clone)]
pub struct DomainEventReader {
    pool: SqlitePool,
}

impl DomainEventReader {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Positions cover the global stream, including pages with no matching events.
    pub async fn read_after(
        &self,
        after: i64,
        application_id: Option<&str>,
        limit: u32,
    ) -> Result<DomainEventPage> {
        if limit == 0 {
            return Err(pontia_core::Error::Domain(
                "Event page size must be positive".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        let head_position: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(position),0) FROM domain_event_positions")
                .fetch_one(&mut *tx)
                .await?;
        let events: Vec<DomainEventRecord> = sqlx::query_as("SELECT p.position,e.event_id,e.session_id,e.turn_id,e.source,e.event_type,e.occurred_at,e.payload,e.created_at,s.application_id FROM domain_event_positions p JOIN events e USING(event_id) LEFT JOIN sessions s USING(session_id) WHERE p.position>? AND (? IS NULL OR s.application_id=?) ORDER BY p.position LIMIT ?")
            .bind(after).bind(application_id).bind(application_id).bind(limit).fetch_all(&mut *tx).await?;
        let next_position = if events.len() < limit as usize {
            head_position
        } else {
            events.last().expect("nonempty full page").position
        };
        tx.commit().await?;
        Ok(DomainEventPage {
            events,
            next_position,
            head_position,
        })
    }
}
