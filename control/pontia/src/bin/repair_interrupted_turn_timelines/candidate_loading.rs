use sqlx::{Row, SqlitePool};

#[derive(Debug)]
pub(super) struct CandidateRow {
    pub(super) event_id: String,
    pub(super) session_id: String,
    pub(super) turn_id: String,
    pub(super) terminal_leaf_id: Option<String>,
    pub(super) turn_state: String,
    pub(super) terminal_event_count: i64,
    pub(super) head_cursor: Option<String>,
    pub(super) event_timeline_boundary: Option<String>,
    pub(super) binding_id: Option<String>,
    pub(super) binding_client_type: Option<String>,
    pub(super) client_session_file: Option<String>,
    pub(super) next_head_cursor: Option<String>,
}

pub(super) async fn load_candidates(pool: &SqlitePool) -> Result<Vec<CandidateRow>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT e.event_id,
               e.session_id,
               e.turn_id,
               json_extract(e.payload, '$.terminal_leaf_id') AS terminal_leaf_id,
               t.state AS turn_state,
               (
                   SELECT COUNT(*)
                   FROM events terminal
                   WHERE terminal.turn_id = e.turn_id
                     AND terminal.session_id = e.session_id
                     AND terminal.event_type IN (
                         'turn.completed',
                         'turn.failed',
                         'turn.interrupted',
                         'turn.dispatch_failed',
                         'turn.abandoned'
                     )
               ) AS terminal_event_count,
               t.head_cursor,
               e.timeline_boundary AS event_timeline_boundary,
               a.id AS binding_id,
               a.client_type AS binding_client_type,
               a.client_session_file,
               (
                   SELECT later.head_cursor
                   FROM turns later
                   WHERE later.session_id = e.session_id
                     AND later.turn_id > e.turn_id
                     AND later.head_cursor IS NOT NULL
                   ORDER BY later.turn_id
                   LIMIT 1
               ) AS next_head_cursor
        FROM events e
        JOIN turns t ON t.turn_id = e.turn_id AND t.session_id = e.session_id
        LEFT JOIN agent_bindings a ON a.session_id = e.session_id
        WHERE e.event_type = 'turn.interrupted'
          AND t.tail_cursor IS NULL
        ORDER BY e.occurred_at, e.event_id
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(CandidateRow {
                event_id: row.try_get("event_id")?,
                session_id: row.try_get("session_id")?,
                turn_id: row.try_get("turn_id")?,
                terminal_leaf_id: row.try_get("terminal_leaf_id")?,
                turn_state: row.try_get("turn_state")?,
                terminal_event_count: row.try_get("terminal_event_count")?,
                head_cursor: row.try_get("head_cursor")?,
                event_timeline_boundary: row.try_get("event_timeline_boundary")?,
                binding_id: row.try_get("binding_id")?,
                binding_client_type: row.try_get("binding_client_type")?,
                client_session_file: row.try_get("client_session_file")?,
                next_head_cursor: row.try_get("next_head_cursor")?,
            })
        })
        .collect()
}
