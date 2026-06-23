use super::*;
use pontia_storage_sqlite::repositories::tasks::SqliteTaskRepository;

impl ExternalQueryService {
    pub async fn list_tasks(&self) -> Result<Vec<TaskView>> {
        let repository = SqliteTaskRepository::new(self.pool.clone());
        let rows = repository.list_tasks().await?;

        rows.into_iter().map(task_row_to_view).collect()
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskView>> {
        let repository = SqliteTaskRepository::new(self.pool.clone());
        let row = repository.get_task(task_id).await?;

        row.map(task_row_to_view).transpose()
    }

    pub async fn list_task_events(&self, task_id: &str) -> Result<Vec<TaskEventView>> {
        let repository = SqliteTaskRepository::new(self.pool.clone());
        let rows = repository.list_task_events(task_id).await?;

        rows.into_iter().map(task_event_row_to_view).collect()
    }
}
