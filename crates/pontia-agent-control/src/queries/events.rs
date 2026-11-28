use pontia_core::error::Result;
use pontia_storage_sqlite::repositories::events::SqliteEventRepository;

use super::ControlQueryService;
use crate::views::events::EventView;

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
}
