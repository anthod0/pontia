use super::*;
use pontia_storage_sqlite::repositories::events::SqliteEventRepository;

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

    pub async fn current_session_stream_rowid(&self) -> Result<i64> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        repository.current_session_stream_rowid().await
    }

    pub async fn current_task_stream_rowid(&self) -> Result<i64> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        repository.current_task_stream_rowid().await
    }

    pub async fn list_session_stream_items_after(
        &self,
        after_rowid: i64,
        limit: i64,
    ) -> Result<Vec<EventStreamItem>> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        let rows = repository
            .list_session_stream_rows_after(after_rowid, limit)
            .await?;

        rows.into_iter().map(event_stream_row_to_item).collect()
    }

    pub async fn list_task_event_stream_items_after(
        &self,
        after_rowid: i64,
        limit: i64,
    ) -> Result<Vec<TaskEventStreamItem>> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        let rows = repository
            .list_task_stream_rows_after(after_rowid, limit)
            .await?;

        rows.into_iter()
            .map(task_event_stream_row_to_item)
            .collect()
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
