use pontia_core::time::utc_now;
use pontia_storage_sqlite::models::workflows::{WorkflowNodeRow, WorkflowRow};
use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::{WorkflowActivePatchView, WorkflowQueryService, graph::ordered_nodes};
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowListItemView {
    pub workflow_id: String,
    pub title: String,
    pub state: String,
    pub current_revision: i64,
    pub failure_message: Option<String>,
    pub agent_submitted_count: usize,
    pub agent_total_count: usize,
    pub current_phase_name: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub elapsed_ms: u64,
    pub observation_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowDetailView {
    pub workflow_id: String,
    pub title: String,
    pub state: String,
    pub current_revision: i64,
    pub definition_file: Option<String>,
    pub active_patch: Option<WorkflowActivePatchView>,
    pub failure_message: Option<String>,
    pub cwd: String,
    pub agent_submitted_count: usize,
    pub agent_total_count: usize,
    pub current_node_id: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub elapsed_ms: u64,
    pub nodes: Vec<WorkflowNodeView>,
    pub retry_failure_event_id: Option<String>,
    pub retry_unavailable_reason: Option<String>,
    pub recoveries: Vec<WorkflowRecoveryView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowRecoveryView {
    #[serde(flatten)]
    pub recovery: pontia_storage_sqlite::models::workflows::WorkflowRecoveryRow,
    pub original_failure_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowNodeView {
    pub node_id: String,
    pub phase: String,
    pub title: String,
    pub status: WorkflowAgentStatus,
    pub session_id: Option<String>,
    pub session_state: Option<String>,
    pub submitted_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAgentStatus {
    Pending,
    Starting,
    Running,
    Paused,
    Idle,
    Exiting,
    Submitted,
    Failed,
    Unknown,
}

impl WorkflowQueryService {
    async fn failure_index(
        &self,
        workflow: &WorkflowRow,
        nodes: &[WorkflowNodeRow],
    ) -> Result<Option<usize>> {
        if workflow.state != "failed" {
            return Ok(None);
        }
        let failed_node_id = self
            .workflows
            .failure_node_id(&workflow.workflow_id)
            .await?;
        Ok(Some(
            failed_node_id
                .and_then(|node_id| nodes.iter().position(|node| node.node_id == node_id))
                .or_else(|| nodes.iter().position(|node| node.submitted_at.is_none()))
                .unwrap_or_else(|| nodes.len().saturating_sub(1)),
        ))
    }

    pub async fn list_workflows(&self, limit: u32) -> Result<Vec<WorkflowListItemView>> {
        let workflows = self.workflows.list_workflows(limit).await?;
        let mut views = Vec::with_capacity(workflows.len());
        for workflow in workflows {
            let nodes = self.workflows.list_nodes(&workflow.workflow_id).await?;
            let submitted = nodes
                .iter()
                .filter(|node| node.submitted_at.is_some())
                .count();
            let total = nodes.len();
            match ordered_nodes(&workflow.workflow_id, nodes) {
                Ok(nodes) => {
                    let failed_index = self.failure_index(&workflow, &nodes).await?;
                    let current = failed_index
                        .and_then(|index| nodes.get(index))
                        .or_else(|| current_node(&nodes));
                    views.push(list_item(
                        workflow,
                        submitted,
                        nodes.len(),
                        current.map(|node| node.phase.clone()),
                        None,
                    ));
                }
                Err(Error::InvalidObservation(_)) => views.push(list_item(
                    workflow,
                    submitted,
                    total,
                    None,
                    Some("invalid_definition".to_string()),
                )),
                Err(error) => return Err(error),
            }
        }
        Ok(views)
    }

    pub async fn get_workflow(&self, workflow_id: &str) -> Result<Option<WorkflowDetailView>> {
        let Some(workflow) = self.workflows.get_workflow(workflow_id).await? else {
            return Ok(None);
        };
        let nodes = ordered_nodes(workflow_id, self.workflows.list_nodes(workflow_id).await?)?;
        let failure_index = self.failure_index(&workflow, &nodes).await?;
        let current_node_id = failure_index
            .and_then(|index| nodes.get(index))
            .or_else(|| current_node(&nodes))
            .map(|node| node.node_id.clone());
        let mut views = Vec::with_capacity(nodes.len());
        for (index, node) in nodes.iter().enumerate() {
            let session = match node.session_id.as_deref() {
                Some(session_id) => self.sessions.get_session(session_id).await?,
                None => None,
            };
            let session_state = session.as_ref().map(|session| session.state.clone());
            let status = derive_status(
                &workflow,
                node,
                session_state.as_deref(),
                failure_index == Some(index),
                current_node_id.as_deref() == Some(node.node_id.as_str()),
            );
            views.push(WorkflowNodeView {
                node_id: node.node_id.clone(),
                phase: node.phase.clone(),
                title: node.title.clone(),
                status,
                session_id: node.session_id.clone(),
                session_state,
                submitted_at: node.submitted_at.clone(),
            });
        }
        let submitted = nodes
            .iter()
            .filter(|node| node.submitted_at.is_some())
            .count();
        let candidate = self.workflows.recovery_candidate(workflow_id).await?;
        let recovery_rows = self.workflows.list_recoveries(workflow_id).await?;
        let events = if recovery_rows.is_empty() {
            Vec::new()
        } else {
            self.workflows.list_events(workflow_id).await?
        };
        let recoveries = recovery_rows
            .into_iter()
            .map(|recovery| {
                let original_failure_message = events
                    .iter()
                    .find(|event| event.event_id == recovery.failure_event_id)
                    .and_then(|event| {
                        serde_json::from_str::<serde_json::Value>(&event.payload).ok()
                    })
                    .and_then(|payload| payload["failure_message"].as_str().map(str::to_string));
                WorkflowRecoveryView {
                    recovery,
                    original_failure_message,
                }
            })
            .collect();
        let retry_unavailable_reason = (workflow.state == "failed" && candidate.is_none()).then(||
            "Retry supports an unsubmitted Pi node after a confirmed Session exit. Its native binding must exist, and pending or uncertain input must be resolved first. Other failure causes require intervention.".to_string());
        Ok(Some(WorkflowDetailView {
            retry_failure_event_id: candidate.map(|candidate| candidate.failure_event_id),
            retry_unavailable_reason,
            recoveries,
            workflow_id: workflow.workflow_id.clone(),
            title: workflow.title.clone(),
            state: workflow.state.clone(),
            current_revision: workflow.current_revision,
            definition_file: None,
            active_patch: None,
            failure_message: workflow.failure_message.clone(),
            cwd: workflow.cwd.clone(),
            agent_submitted_count: submitted,
            agent_total_count: nodes.len(),
            current_node_id,
            started_at: workflow.started_at.clone(),
            completed_at: workflow.completed_at.clone(),
            created_at: workflow.created_at.clone(),
            updated_at: workflow.updated_at.clone(),
            elapsed_ms: elapsed_ms(&workflow),
            nodes: views,
        }))
    }
}

fn list_item(
    workflow: WorkflowRow,
    submitted: usize,
    total: usize,
    current_phase_name: Option<String>,
    observation_error: Option<String>,
) -> WorkflowListItemView {
    WorkflowListItemView {
        workflow_id: workflow.workflow_id.clone(),
        title: workflow.title.clone(),
        state: workflow.state.clone(),
        current_revision: workflow.current_revision,
        failure_message: workflow.failure_message.clone(),
        agent_submitted_count: submitted,
        agent_total_count: total,
        current_phase_name,
        started_at: workflow.started_at.clone(),
        completed_at: workflow.completed_at.clone(),
        created_at: workflow.created_at.clone(),
        updated_at: workflow.updated_at.clone(),
        elapsed_ms: elapsed_ms(&workflow),
        observation_error,
    }
}

fn current_node(nodes: &[WorkflowNodeRow]) -> Option<&WorkflowNodeRow> {
    nodes
        .iter()
        .find(|node| node.submitted_at.is_none())
        .or_else(|| nodes.last())
}

fn derive_status(
    workflow: &WorkflowRow,
    node: &WorkflowNodeRow,
    session_state: Option<&str>,
    failure_location: bool,
    current: bool,
) -> WorkflowAgentStatus {
    if node.session_id.is_some() && session_state.is_none() {
        return WorkflowAgentStatus::Unknown;
    }
    if failure_location {
        return WorkflowAgentStatus::Failed;
    }
    // An unsubmitted requester is suspended during Patch planning and queued
    // continuation. A submitted Node still exits unless the Workflow is paused.
    if current
        && session_state == Some("interrupted")
        && (workflow.state == "paused"
            || (workflow.state != "failed" && node.submitted_at.is_none()))
    {
        return WorkflowAgentStatus::Paused;
    }
    if node.submitted_at.is_some() {
        return if session_state == Some("exited") || workflow.state == "completed" {
            WorkflowAgentStatus::Submitted
        } else {
            WorkflowAgentStatus::Exiting
        };
    }
    if node.session_id.is_none() {
        return WorkflowAgentStatus::Pending;
    }
    match session_state {
        Some("created" | "starting") => WorkflowAgentStatus::Starting,
        Some("busy") => WorkflowAgentStatus::Running,
        Some("interrupted" | "error" | "exited") => WorkflowAgentStatus::Failed,
        Some("idle") if workflow.state == "idle" && current => WorkflowAgentStatus::Idle,
        Some("idle") => WorkflowAgentStatus::Running,
        _ => WorkflowAgentStatus::Unknown,
    }
}

fn elapsed_ms(workflow: &WorkflowRow) -> u64 {
    let Some(started) = parse_time(workflow.started_at.as_deref()) else {
        return 0;
    };
    let end = match workflow.state.as_str() {
        "completed" => parse_time(workflow.completed_at.as_deref())
            .or_else(|| parse_time(Some(&workflow.updated_at))),
        "failed" | "idle" => parse_time(Some(&workflow.updated_at)),
        _ => Some(utc_now()),
    }
    .unwrap_or(started);
    u64::try_from((end - started).whole_milliseconds().max(0)).unwrap_or(u64::MAX)
}

fn parse_time(value: Option<&str>) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value?, &Rfc3339).ok()
}

#[cfg(test)]
mod tests;
