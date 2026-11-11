use pontia_agent_control::{client_contract::ClientOperation, sessions::SessionEnvironment};
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use std::collections::BTreeMap;

pub struct WorkflowSessionEnvironment {
    repository: SqliteWorkflowRepository,
    pool: sqlx::SqlitePool,
}
impl WorkflowSessionEnvironment {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            repository: SqliteWorkflowRepository::new(pool.clone()),
            pool,
        }
    }
}
impl SessionEnvironment for WorkflowSessionEnvironment {
    fn environment<'a>(
        &'a self,
        session_id: &'a str,
    ) -> ClientOperation<'a, BTreeMap<String, String>> {
        Box::pin(async move {
            let replanner: Option<(String, String)> = sqlx::query_as(
                "SELECT json_extract(metadata, '$.workflow_id'),
                        json_extract(metadata, '$.workflow_patch_id')
                 FROM sessions WHERE session_id = ?
                   AND json_extract(metadata, '$.role') = 'workflow_replanner'",
            )
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await?;
            if let Some((workflow_id, patch_id)) = replanner {
                return Ok(BTreeMap::from([
                    ("PONTIA_WORKFLOW_ID".into(), workflow_id),
                    ("PONTIA_WORKFLOW_PATCH_ID".into(), patch_id),
                ]));
            }
            let workflow_id = self
                .repository
                .get_node_by_session(session_id)
                .await?
                .map(|node| node.workflow_id);
            Ok(workflow_id
                .map(|workflow_id| {
                    BTreeMap::from([("PONTIA_WORKFLOW_ID".to_string(), workflow_id)])
                })
                .unwrap_or_default())
        })
    }
}
