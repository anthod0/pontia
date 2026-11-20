use pontia_core::{Result, domain::EventType};

use super::ControlQueryService;

/// Bounds are exclusive. Timestamps are UTC milliseconds, with binary ID order for ties.
#[derive(Clone, Copy)]
pub enum TimePage<'a> {
    Latest,
    Before(i64, &'a str),
    After(i64, &'a str),
}

#[derive(sqlx::FromRow)]
pub struct SessionEventSummary {
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: String,
    pub timeline_ms: i64,
}

fn event_summary_query(kinds: &[EventType]) -> String {
    // Enum literals let SQLite match a partial index for the requested event types.
    let types = kinds
        .iter()
        .map(|kind| format!("'{kind}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT event_id, event_type, occurred_at, timeline_ms FROM events
             WHERE session_id = ? AND event_type IN ({types})"
    )
}

impl ControlQueryService {
    pub async fn session_event_page(
        &self,
        session_id: &str,
        kinds: &[EventType],
        page: TimePage<'_>,
        limit: u32,
    ) -> Result<Vec<SessionEventSummary>> {
        let mut query = event_summary_query(kinds);
        match page {
            TimePage::Latest => {}
            TimePage::Before(_, _) => {
                query.push_str(" AND (timeline_ms, event_id) < (?, ?)");
            }
            TimePage::After(_, _) => {
                query.push_str(" AND (timeline_ms, event_id) > (?, ?)");
            }
        }
        query.push_str(if matches!(page, TimePage::After(..)) {
            " ORDER BY timeline_ms, event_id LIMIT ?"
        } else {
            " ORDER BY timeline_ms DESC, event_id DESC LIMIT ?"
        });
        let mut rows = sqlx::query_as(&query).bind(session_id);
        if let TimePage::Before(ms, id) | TimePage::After(ms, id) = page {
            rows = rows.bind(ms).bind(id);
        }
        Ok(rows.bind(limit).fetch_all(&self.pool).await?)
    }

    pub async fn session_event_summary(
        &self,
        session_id: &str,
        kinds: &[EventType],
        event_id: &str,
    ) -> Result<Option<SessionEventSummary>> {
        Ok(
            sqlx::query_as(&format!("{} AND event_id = ?", event_summary_query(kinds)))
                .bind(session_id)
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
}
