use pontia_agent_control::EventStreamScope;
use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::events::SqliteEventRepository;

use super::ExternalQueryService;
use crate::views::events::{EventStreamItem, row_to_item};

impl ExternalQueryService {
    pub async fn current_session_stream_rowid(&self) -> Result<i64> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        repository.current_session_stream_rowid().await
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

        self.enrich_file_events(
            rows.into_iter()
                .map(row_to_item)
                .collect::<Result<Vec<_>>>()?,
        )
        .await
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

        self.enrich_file_events(
            rows.into_iter()
                .map(row_to_item)
                .collect::<Result<Vec<_>>>()?,
        )
        .await
    }
    async fn enrich_file_events(
        &self,
        mut items: Vec<EventStreamItem>,
    ) -> Result<Vec<EventStreamItem>> {
        let files = pontia_storage_sqlite::repositories::session_files::SessionFileRepository::new(
            self.pool.clone(),
        );
        for item in &mut items {
            if item.event.fact.event_type == "session.file_shared" {
                let id = item.event.fact.payload["file_id"]
                    .as_str()
                    .ok_or_else(|| Error::Domain("File event missing file_id".into()))?;
                item.event.file = Some(
                    files
                        .get(&item.event.fact.session_id, id)
                        .await?
                        .ok_or_else(|| Error::NotFound(format!("Shared file {id} not found")))?,
                );
            }
        }
        Ok(items)
    }
}
