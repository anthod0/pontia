use super::*;

impl ExternalQueryService {
    pub async fn list_session_events(&self, session_id: &str) -> Result<Vec<EventView>> {
        let rows = sqlx::query(
            r#"SELECT event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_event_view).collect()
    }

    pub async fn list_turn_events(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Vec<EventView>> {
        let rows = sqlx::query(
            r#"SELECT event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? AND turn_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .bind(turn_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_event_view).collect()
    }

    pub async fn resolve_event_cursor(
        &self,
        scope: EventStreamScope<'_>,
        after_event_id: &str,
    ) -> Result<i64> {
        let row = match scope {
            EventStreamScope::Session { session_id } => {
                sqlx::query("SELECT rowid FROM events WHERE session_id = ? AND event_id = ?")
                    .bind(session_id)
                    .bind(after_event_id)
                    .fetch_optional(&self.pool)
                    .await?
            }
            EventStreamScope::Turn {
                session_id,
                turn_id,
            } => sqlx::query(
                "SELECT rowid FROM events WHERE session_id = ? AND turn_id = ? AND event_id = ?",
            )
            .bind(session_id)
            .bind(turn_id)
            .bind(after_event_id)
            .fetch_optional(&self.pool)
            .await?,
        };

        let Some(row) = row else {
            return Err(Error::Domain(format!(
                "event cursor {after_event_id} is not valid for requested stream"
            )));
        };

        Ok(row.try_get("rowid")?)
    }

    pub async fn current_dashboard_stream_cursor(&self) -> Result<DashboardStreamCursor> {
        let session_rowid = sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(rowid) FROM events")
            .fetch_one(&self.pool)
            .await?
            .unwrap_or(0);
        let task_rowid = sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(rowid) FROM task_events")
            .fetch_one(&self.pool)
            .await?
            .unwrap_or(0);
        Ok(DashboardStreamCursor {
            session_rowid,
            task_rowid,
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
        let session_rows = sqlx::query(
            r#"SELECT rowid, event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE rowid > ? ORDER BY rowid LIMIT ?"#,
        )
        .bind(cursor.session_rowid)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let task_rows = sqlx::query(
            r#"SELECT rowid, event_id, task_id, event_type, payload, created_at
               FROM task_events WHERE rowid > ? ORDER BY rowid LIMIT ?"#,
        )
        .bind(cursor.task_rowid)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut items = Vec::new();
        for row in session_rows {
            let rowid = row.try_get("rowid")?;
            let event = row_to_event_view(row)?;
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
            let rowid = row.try_get("rowid")?;
            let event = row_to_task_event_view(row)?;
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
        let rows = match scope {
            EventStreamScope::Session { session_id } => {
                sqlx::query(
                    r#"SELECT rowid, event_id, session_id, turn_id, source, event_type, occurred_at, payload
                       FROM events WHERE session_id = ? AND rowid > ? ORDER BY rowid LIMIT ?"#,
                )
                .bind(session_id)
                .bind(after_rowid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            EventStreamScope::Turn {
                session_id,
                turn_id,
            } => {
                sqlx::query(
                    r#"SELECT rowid, event_id, session_id, turn_id, source, event_type, occurred_at, payload
                       FROM events WHERE session_id = ? AND turn_id = ? AND rowid > ? ORDER BY rowid LIMIT ?"#,
                )
                .bind(session_id)
                .bind(turn_id)
                .bind(after_rowid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };

        rows.into_iter().map(row_to_event_stream_item).collect()
    }
}
