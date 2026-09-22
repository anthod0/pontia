use pontia_application::{
    ExternalQueryService, InboxCommandService, RuntimeControlService, SubmitInboxMessageRequest,
};
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowControlOutcome {
    pub workflow_id: String,
    pub state: String,
    pub interrupt_requested: bool,
    pub continue_sent: bool,
}

#[derive(Clone)]
pub struct WorkflowControlService {
    pool: SqlitePool,
    event_ingest: pontia_application::EventIngestService,
    pi_control: Option<pontia_application::PiControlService>,
    workflows: SqliteWorkflowRepository,
}

impl WorkflowControlService {
    pub fn new(event_ingest: pontia_application::EventIngestService) -> Self {
        let pool = event_ingest.db();
        Self {
            workflows: SqliteWorkflowRepository::new(pool.clone()),
            pool,
            event_ingest,
            pi_control: None,
        }
    }

    pub fn with_pi_control(mut self, control: pontia_application::PiControlService) -> Self {
        self.pi_control = Some(control);
        self
    }

    pub async fn pause(&self, workflow_id: &str) -> Result<WorkflowControlOutcome> {
        self.require_state(workflow_id, "running").await?;
        self.workflows
            .pause_workflow(workflow_id, &Uuid::now_v7().to_string())
            .await?;

        let mut interrupt_requested = false;
        if let Some(session_id) = self.current_session_id(workflow_id).await? {
            let session = ExternalQueryService::new(self.pool.clone())
                .get_session(&session_id)
                .await?;
            if session
                .as_ref()
                .is_some_and(|session| session.state == "busy")
            {
                RuntimeControlService::new(self.event_ingest.clone())
                    .interrupt_current_turn(&session_id)
                    .await?;
                interrupt_requested = true;
            }
        }

        Ok(WorkflowControlOutcome {
            workflow_id: workflow_id.to_string(),
            state: "paused".to_string(),
            interrupt_requested,
            continue_sent: false,
        })
    }

    pub async fn resume(&self, workflow_id: &str) -> Result<WorkflowControlOutcome> {
        self.require_state(workflow_id, "paused").await?;
        let mut continue_sent = false;
        if let Some(session_id) = self.current_session_id(workflow_id).await? {
            let session = ExternalQueryService::new(self.pool.clone())
                .get_session(&session_id)
                .await?;
            if session
                .as_ref()
                .is_some_and(|session| session.state == "interrupted")
            {
                let mut inbox = InboxCommandService::new(self.event_ingest.clone());
                if let Some(control) = &self.pi_control {
                    inbox = inbox.with_pi_control(control.clone());
                }
                let outcome = inbox
                    .submit_message(
                        &session_id,
                        SubmitInboxMessageRequest {
                            input: "continue".to_string(),
                            delivery_policy: "after_idle".to_string(),
                            branch_target_turn_id: None,
                            metadata: json!({
                                "source": "workflow_resume",
                                "workflow_id": workflow_id,
                            }),
                        },
                    )
                    .await?;
                if outcome.data["inbox_message"]["state"] == "failed" {
                    let message = outcome.data["inbox_message"]["failure_message"]
                        .as_str()
                        .unwrap_or("continue dispatch failed");
                    return Err(pontia_core::Error::Domain(format!(
                        "workflow {workflow_id} could not send continue while resuming: {message}"
                    ))
                    .into());
                }
                continue_sent = true;
            }
        }

        self.workflows
            .resume_workflow(workflow_id, &Uuid::now_v7().to_string())
            .await?;
        Ok(WorkflowControlOutcome {
            workflow_id: workflow_id.to_string(),
            state: "running".to_string(),
            interrupt_requested: false,
            continue_sent,
        })
    }

    async fn current_session_id(&self, workflow_id: &str) -> Result<Option<String>> {
        let nodes = self.workflows.list_nodes(workflow_id).await?;
        Ok(nodes
            .iter()
            .find(|node| node.submitted_at.is_none())
            .or_else(|| nodes.last())
            .and_then(|node| node.session_id.clone()))
    }

    async fn require_state(&self, workflow_id: &str, expected: &str) -> Result<()> {
        let workflow = self
            .workflows
            .get_workflow(workflow_id)
            .await?
            .ok_or_else(|| Error::WorkflowNotFound(workflow_id.to_string()))?;
        if workflow.state != expected {
            return Err(pontia_core::Error::StateConflict(format!(
                "workflow {workflow_id} must be {expected}, but is {}",
                workflow.state
            ))
            .into());
        }
        Ok(())
    }
}
