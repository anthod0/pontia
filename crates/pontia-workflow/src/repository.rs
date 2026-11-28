use pontia_agent_control::ControlQueryService;
use pontia_agent_control::{AgentControl, DurableSubscription, SubscriptionConfig};
use projection::WorkflowProjectionHandler;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Notify;
mod projection;
mod terminal;

mod definition;
mod events;
mod lifecycle;
mod nodes;
mod queries;
mod recovery;
mod reports;
pub use recovery::WorkflowRecoveryCandidate;

#[derive(Debug, Clone)]
pub struct CreateWorkflowRecord {
    pub initiator_session_id: String,
    pub workflow_id: String,
    pub title: String,
    pub cwd: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct CreateWorkflowNodeRecord {
    pub node_id: String,
    pub workflow_id: String,
    pub parent_node_id: Option<String>,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    pub inputs: String,
    pub output: String,
    pub execution_profile_id: Option<String>,
}

#[derive(Clone)]
pub struct WorkflowState {
    pool: SqlitePool,
    pub(crate) queries: ControlQueryService,
    pub(crate) projection: Arc<DurableSubscription>,
    pub(crate) changed: Arc<Notify>,
    recovery_client_types: Vec<String>,
}

impl WorkflowState {
    pub async fn new(pool: SqlitePool, control: &AgentControl) -> pontia_core::Result<Self> {
        let queries = control.queries();
        let changed = Arc::new(Notify::new());
        let handler = Arc::new(WorkflowProjectionHandler {
            pool: pool.clone(),
            queries: queries.clone(),
            changed: changed.clone(),
        });
        let projection = control
            .events()
            .register_durable(
                SubscriptionConfig::new("workflow.projection"),
                move |event| {
                    let handler = handler.clone();
                    async move { handler.apply(event).await }
                },
            )
            .await?;
        let recovery_client_types = control
            .clients()
            .registrations()
            .filter(|client| client.spec.adapter.lifecycle.confirmed_exit_recovery)
            .map(|client| client.spec.client_type.to_owned())
            .collect();
        Ok(Self {
            pool,
            queries,
            changed,
            projection: Arc::new(projection),
            recovery_client_types,
        })
    }

    pub async fn catch_up(&self) -> pontia_core::Result<()> {
        self.projection.catch_up().await
    }
}
