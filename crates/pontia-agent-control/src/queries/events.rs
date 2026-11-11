use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::events::SqliteEventRepository;

use super::ControlQueryService;
use crate::views::events::{EventStreamScope, EventView};

impl ControlQueryService {
    pub async fn list_session_events(&self, session_id: &str) -> Result<Vec<EventView>> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        let rows = repository.list_session_events(session_id).await?;

        rows.into_iter().map(EventView::try_from).collect()
    }

    pub async fn list_turn_events(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Vec<EventView>> {
        let repository = SqliteEventRepository::new(self.pool.clone());
        let rows = repository.list_turn_events(session_id, turn_id).await?;

        rows.into_iter().map(EventView::try_from).collect()
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
}
