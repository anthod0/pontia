use super::*;
use crate::storage::sqlite::repositories::events::SqliteEventRepository;

impl ExternalQueryService {
    pub async fn list_session_events(&self, session_id: &str) -> Result<Vec<EventView>> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        let rows = repository.list_session_events(session_id).await?;

        rows.into_iter().map(event_row_to_view).collect()
    }

    pub async fn list_turn_events(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Vec<EventView>> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        let rows = repository.list_turn_events(session_id, turn_id).await?;

        rows.into_iter().map(event_row_to_view).collect()
    }

    pub async fn resolve_event_cursor(
        &self,
        scope: EventStreamScope<'_>,
        after_event_id: &str,
    ) -> Result<i64> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        let rowid = match scope {
            EventStreamScope::Session { session_id } => {
                repository
                    .resolve_session_event_cursor(session_id, after_event_id)
                    .await?
            }
            EventStreamScope::Turn {
                session_id,
                turn_id,
            } => {
                repository
                    .resolve_turn_event_cursor(session_id, turn_id, after_event_id)
                    .await?
            }
        };

        rowid.ok_or_else(|| {
            Error::Domain(format!(
                "event cursor {after_event_id} is not valid for requested stream"
            ))
        })
    }

    pub async fn current_dashboard_stream_cursor(&self) -> Result<DashboardStreamCursor> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        Ok(DashboardStreamCursor {
            session_rowid: repository.current_session_stream_rowid().await?,
            task_rowid: repository.current_task_stream_rowid().await?,
        })
    }

    pub fn parse_dashboard_stream_cursor(&self, cursor: &str) -> Result<DashboardStreamCursor> {
        let mut session_rowid = None;
        let mut task_rowid = None;
        for part in cursor.split(';') {
            let Some((name, value)) = part.split_once(':') else {
                return Err(Error::Domain(format!(
                    "dashboard cursor {cursor} is invalid"
                )));
            };
            let rowid = value
                .parse::<i64>()
                .map_err(|_| Error::Domain(format!("dashboard cursor {cursor} is invalid")))?;
            match name {
                "session" => session_rowid = Some(rowid),
                "task" => task_rowid = Some(rowid),
                _ => {
                    return Err(Error::Domain(format!(
                        "dashboard cursor {cursor} is invalid"
                    )));
                }
            }
        }
        Ok(DashboardStreamCursor {
            session_rowid: session_rowid.unwrap_or(0),
            task_rowid: task_rowid.unwrap_or(0),
        })
    }

    pub async fn list_dashboard_stream_items_after(
        &self,
        cursor: DashboardStreamCursor,
        limit: i64,
    ) -> Result<Vec<DashboardStreamItem>> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        let session_rows = repository
            .list_session_stream_rows_after(cursor.session_rowid, limit)
            .await?;
        let task_rows = repository
            .list_task_stream_rows_after(cursor.task_rowid, limit)
            .await?;

        let mut items = Vec::new();
        for row in session_rows {
            let rowid = row.rowid;
            let event = event_stream_row_to_view(row)?;
            let occurred_at = event.time.clone();
            items.push(DashboardStreamItem {
                cursor: DashboardStreamCursor {
                    session_rowid: rowid,
                    task_rowid: cursor.task_rowid,
                },
                occurred_at: occurred_at.clone(),
                event: DashboardStreamEvent::SessionEvent {
                    id: event.event_id.clone(),
                    occurred_at,
                    event,
                },
            });
        }
        for row in task_rows {
            let rowid = row.rowid;
            let event = task_event_stream_row_to_view(row)?;
            let occurred_at = event.created_at.clone();
            items.push(DashboardStreamItem {
                cursor: DashboardStreamCursor {
                    session_rowid: cursor.session_rowid,
                    task_rowid: rowid,
                },
                occurred_at: occurred_at.clone(),
                event: DashboardStreamEvent::TaskEvent {
                    id: event.event_id.clone(),
                    occurred_at,
                    event,
                },
            });
        }

        items.sort_by(|a, b| {
            a.occurred_at
                .cmp(&b.occurred_at)
                .then(a.cursor.session_rowid.cmp(&b.cursor.session_rowid))
                .then(a.cursor.task_rowid.cmp(&b.cursor.task_rowid))
        });
        items.truncate(limit as usize);
        let mut running = cursor;
        for item in &mut items {
            running.session_rowid = running.session_rowid.max(item.cursor.session_rowid);
            running.task_rowid = running.task_rowid.max(item.cursor.task_rowid);
            item.cursor = running;
        }
        Ok(items)
    }

    pub async fn list_event_stream_items_after(
        &self,
        scope: EventStreamScope<'_>,
        after_rowid: i64,
        limit: i64,
    ) -> Result<Vec<EventStreamItem>> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        let rows = match scope {
            EventStreamScope::Session { session_id } => {
                repository
                    .list_session_event_stream_rows_after(session_id, after_rowid, limit)
                    .await?
            }
            EventStreamScope::Turn {
                session_id,
                turn_id,
            } => {
                repository
                    .list_turn_event_stream_rows_after(session_id, turn_id, after_rowid, limit)
                    .await?
            }
        };

        rows.into_iter().map(event_stream_row_to_item).collect()
    }
}
