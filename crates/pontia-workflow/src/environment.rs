use pontia_agent_control::{client_contract::ClientOperation, sessions::SessionEnvironment};
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use std::collections::BTreeMap;

pub struct WorkflowSessionEnvironment {
    repository: SqliteWorkflowRepository,
}
impl WorkflowSessionEnvironment {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            repository: SqliteWorkflowRepository::new(pool),
        }
    }
}
impl SessionEnvironment for WorkflowSessionEnvironment {
    fn environment<'a>(
        &'a self,
        session_id: &'a str,
    ) -> ClientOperation<'a, BTreeMap<String, String>> {
        Box::pin(async move {
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
