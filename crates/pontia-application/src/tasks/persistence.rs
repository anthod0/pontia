use pontia_core::{error::Result, ids::new_event_id};
use pontia_storage_sqlite::repositories::tasks::SqliteTaskRepository;
use serde_json::Value;

use super::TaskCommandService;

impl TaskCommandService {
    pub(super) async fn record_task_event(
        &self,
        task_id: &str,
        event_type: &str,
        payload: Value,
    ) -> Result<()> {
        let event_id = new_event_id().to_string();
        let payload = serde_json::to_string(&payload)?;
        SqliteTaskRepository::new(self.pool.clone())
            .record_task_event(&event_id, task_id, event_type, &payload)
            .await?;
        Ok(())
    }
}
