use pontia_core::Result;
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};
use std::num::NonZeroU32;

#[derive(Clone, Debug, Default)]
pub enum EventScope {
    #[default]
    All,
    Application(String),
    Session(String),
    Turn {
        session_id: String,
        turn_id: String,
    },
}

impl EventScope {
    fn restrict<'a>(&'a self, query: &mut QueryBuilder<'a, Sqlite>) {
        match self {
            Self::All => {}
            Self::Application(id) => {
                query.push(" AND s.application_id = ").push_bind(id);
            }
            Self::Session(id) => {
                query.push(" AND e.session_id = ").push_bind(id);
            }
            Self::Turn {
                session_id,
                turn_id,
            } => {
                query
                    .push(" AND e.session_id = ")
                    .push_bind(session_id)
                    .push(" AND e.turn_id = ")
                    .push_bind(turn_id);
            }
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct PositionedEvent {
    pub position: i64,
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    pub client_type: String,
    pub event_type: String,
    pub occurred_at: String,
    pub payload: String,
    pub created_at: String,
    pub application_id: Option<String>,
    pub application_ref: Option<String>,
}

pub struct EventWindow {
    pub events: Vec<PositionedEvent>,
    pub end_position: i64,
}

#[derive(Clone)]
pub struct EventSubscriptionRepository {
    pool: SqlitePool,
}

// CROSS JOIN keeps the bounded position range as the outer loop for selective scopes.
const EVENT_TABLES: &str = " FROM domain_event_positions p CROSS JOIN events e USING(event_id)
                             LEFT JOIN sessions s USING(session_id) ";

fn restrict_types<'a>(query: &mut QueryBuilder<'a, Sqlite>, types: &'a [String]) {
    query.push(" AND e.event_type IN (");
    let mut list = query.separated(", ");
    for ty in types {
        list.push_bind(ty);
    }
    list.push_unseparated(")");
}

impl EventSubscriptionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn register(&self, id: &str) -> Result<()> {
        sqlx::query("INSERT INTO event_subscriptions(subscriber_id) VALUES (?) ON CONFLICT(subscriber_id) DO NOTHING")
            .bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn cursor(&self, id: &str) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT cursor_position FROM event_subscriptions WHERE subscriber_id=?",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn advance(&self, id: &str, position: i64) -> Result<()> {
        sqlx::query("UPDATE event_subscriptions SET cursor_position=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE subscriber_id=? AND cursor_position < ?")
            .bind(position).bind(id).bind(position).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn current_head_position(&self) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COALESCE(MAX(position),0) FROM domain_event_positions")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    /// The caller captures a committed head before scanning. Progress counts global
    /// positions even when the scope matches no rows.
    pub async fn scan(
        &self,
        scope: &EventScope,
        event_types: &[String],
        after: i64,
        head: i64,
        window: NonZeroU32,
    ) -> Result<EventWindow> {
        let end_position = after.saturating_add(i64::from(window.get())).min(head);
        let mut query = QueryBuilder::new(
            "SELECT p.position,e.event_id,e.session_id,e.turn_id,e.source,e.client_type,e.event_type,e.occurred_at,e.payload,e.created_at,s.application_id,s.application_ref",
        );
        query
            .push(EVENT_TABLES)
            .push(" WHERE p.position > ")
            .push_bind(after)
            .push(" AND p.position <= ")
            .push_bind(end_position);
        scope.restrict(&mut query);
        if !event_types.is_empty() {
            restrict_types(&mut query, event_types);
        }
        query.push(" ORDER BY p.position");
        let events = query.build_query_as().fetch_all(&self.pool).await?;
        Ok(EventWindow {
            events,
            end_position,
        })
    }

    pub async fn resolve_event_position(
        &self,
        scope: &EventScope,
        event_id: &str,
    ) -> Result<Option<i64>> {
        let mut query = QueryBuilder::new("SELECT p.position");
        query
            .push(EVENT_TABLES)
            .push(" WHERE p.event_id = ")
            .push_bind(event_id);
        scope.restrict(&mut query);
        Ok(query
            .build_query_scalar()
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn has_session_event_after(
        &self,
        session_id: &str,
        after_position: i64,
        event_types: &[String],
    ) -> Result<bool> {
        let mut query = QueryBuilder::new(
            "SELECT EXISTS(SELECT 1 FROM events e JOIN domain_event_positions p USING(event_id) WHERE e.session_id = ",
        );
        query
            .push_bind(session_id)
            .push(" AND p.position > ")
            .push_bind(after_position);
        restrict_types(&mut query, event_types);
        query.push(")");
        Ok(query.build_query_scalar().fetch_one(&self.pool).await?)
    }
}
