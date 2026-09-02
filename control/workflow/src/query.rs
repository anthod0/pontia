use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use pontia_core::time::utc_now;
use pontia_storage_sqlite::{
    models::workflows::{WorkflowNodeRow, WorkflowRow},
    repositories::{sessions::SqliteSessionRepository, workflows::SqliteWorkflowRepository},
};
use serde::Serialize;
use sqlx::SqlitePool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{Error, Result, validation::validate_handoff_file_name};

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

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowContextView {
    pub workflow: WorkflowDetailView,
    pub current_node: WorkflowNodeContextView,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowGraphRevisionView {
    pub workflow_id: String,
    pub revision: i64,
    pub current: bool,
    pub nodes: Vec<WorkflowGraphNodeView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowGraphNodeView {
    pub node_id: String,
    pub parent_node_id: Option<String>,
    pub node_type: String,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    pub inputs: Vec<String>,
    pub output: String,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    pub introduced_revision: i64,
    pub retired_revision: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowNodeContextView {
    pub instructions: String,
    pub inputs: Vec<WorkflowInputView>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowInputView {
    pub name: String,
    pub content: Option<String>,
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

#[derive(Debug, Clone)]
pub struct WorkflowQueryService {
    workflows: SqliteWorkflowRepository,
    sessions: SqliteSessionRepository,
}

impl WorkflowQueryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            workflows: SqliteWorkflowRepository::new(pool.clone()),
            sessions: SqliteSessionRepository::new(pool),
        }
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
                    let current = current_node(&nodes);
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
        let current_node_id = current_node(&nodes).map(|node| node.node_id.clone());
        let failure_index = failure_index(&workflow, &nodes);
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
        Ok(Some(WorkflowDetailView {
            workflow_id: workflow.workflow_id.clone(),
            title: workflow.title.clone(),
            state: workflow.state.clone(),
            current_revision: workflow.current_revision,
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

    pub async fn get_workflow_revision(
        &self,
        workflow_id: &str,
        revision: i64,
    ) -> Result<Option<WorkflowGraphRevisionView>> {
        let Some(workflow) = self.workflows.get_workflow(workflow_id).await? else {
            return Ok(None);
        };
        let nodes = ordered_nodes(
            workflow_id,
            self.workflows
                .list_nodes_at_revision(workflow_id, revision)
                .await?,
        )?;
        let mut views = Vec::with_capacity(nodes.len());
        for node in nodes {
            views.push(WorkflowGraphNodeView {
                node_id: node.node_id,
                parent_node_id: node.parent_node_id,
                node_type: node.node_type,
                phase: node.phase,
                title: node.title,
                instructions: node.instructions,
                inputs: serde_json::from_str(&node.inputs)?,
                output: node.output,
                execution_profile_id: node.execution_profile_id,
                execution_profile_version: node.execution_profile_version,
                introduced_revision: node.introduced_revision,
                retired_revision: node.retired_revision,
            });
        }
        Ok(Some(WorkflowGraphRevisionView {
            workflow_id: workflow.workflow_id,
            revision,
            current: revision == workflow.current_revision,
            nodes: views,
        }))
    }

    pub async fn get_workflow_context(
        &self,
        workflow_id: &str,
        pontia_home: &Path,
    ) -> Result<Option<WorkflowContextView>> {
        let Some(workflow) = self.get_workflow(workflow_id).await? else {
            return Ok(None);
        };
        let current_node_id = workflow
            .current_node_id
            .as_deref()
            .ok_or_else(|| Error::InvalidObservation(workflow_id.to_string()))?;
        let node = self
            .workflows
            .get_node(current_node_id)
            .await?
            .filter(|node| node.workflow_id == workflow_id)
            .ok_or_else(|| Error::InvalidObservation(workflow_id.to_string()))?;
        let input_names: Vec<String> = serde_json::from_str(&node.inputs)?;
        let handoff_dir = pontia_home
            .join("workflows")
            .join(workflow_id)
            .join("handoff");
        let mut inputs = Vec::with_capacity(input_names.len());
        for name in input_names {
            validate_handoff_file_name(&name)?;
            let content = match tokio::fs::read(handoff_dir.join(&name)).await {
                Ok(bytes) => String::from_utf8(bytes).ok(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            inputs.push(WorkflowInputView { name, content });
        }
        Ok(Some(WorkflowContextView {
            workflow,
            current_node: WorkflowNodeContextView {
                instructions: node.instructions,
                inputs,
                output: node.output,
            },
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

fn ordered_nodes(workflow_id: &str, nodes: Vec<WorkflowNodeRow>) -> Result<Vec<WorkflowNodeRow>> {
    if nodes.is_empty()
        || nodes
            .iter()
            .any(|node| node.phase.trim().is_empty() || node.node_type != "agent")
    {
        return Err(Error::InvalidObservation(workflow_id.to_string()));
    }
    let ids: HashSet<&str> = nodes.iter().map(|node| node.node_id.as_str()).collect();
    let roots: Vec<&WorkflowNodeRow> = nodes
        .iter()
        .filter(|node| node.parent_node_id.is_none())
        .collect();
    if roots.len() != 1
        || nodes.iter().any(|node| {
            node.parent_node_id
                .as_deref()
                .is_some_and(|parent| !ids.contains(parent))
        })
    {
        return Err(Error::InvalidObservation(workflow_id.to_string()));
    }

    let mut children: HashMap<&str, &WorkflowNodeRow> = HashMap::new();
    for node in &nodes {
        if let Some(parent) = node.parent_node_id.as_deref()
            && children.insert(parent, node).is_some()
        {
            return Err(Error::InvalidObservation(workflow_id.to_string()));
        }
    }

    let by_id: HashMap<&str, &WorkflowNodeRow> = nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect();
    let mut ordered = Vec::with_capacity(nodes.len());
    let mut seen = HashSet::new();
    let mut next = Some(roots[0].node_id.as_str());
    while let Some(node_id) = next {
        if !seen.insert(node_id) {
            return Err(Error::InvalidObservation(workflow_id.to_string()));
        }
        let Some(node) = by_id.get(node_id) else {
            return Err(Error::InvalidObservation(workflow_id.to_string()));
        };
        ordered.push((*node).clone());
        next = children.get(node_id).map(|child| child.node_id.as_str());
    }
    if ordered.len() != nodes.len() {
        return Err(Error::InvalidObservation(workflow_id.to_string()));
    }
    Ok(ordered)
}

fn current_node(nodes: &[WorkflowNodeRow]) -> Option<&WorkflowNodeRow> {
    nodes
        .iter()
        .find(|node| node.submitted_at.is_none())
        .or_else(|| nodes.last())
}

fn failure_index(workflow: &WorkflowRow, nodes: &[WorkflowNodeRow]) -> Option<usize> {
    (workflow.state == "failed").then(|| {
        nodes
            .iter()
            .position(|node| node.submitted_at.is_none())
            .unwrap_or_else(|| nodes.len().saturating_sub(1))
    })
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
    if workflow.state == "paused" && current && session_state == Some("interrupted") {
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
mod tests {
    use super::*;

    fn workflow(state: &str) -> WorkflowRow {
        WorkflowRow {
            workflow_id: "wf_test".to_string(),
            title: "Test".to_string(),
            cwd: "/tmp".to_string(),
            state: state.to_string(),
            current_revision: 1,
            failure_message: None,
            created_at: "2026-08-14T00:00:00Z".to_string(),
            updated_at: "2026-08-14T00:01:00Z".to_string(),
            started_at: Some("2026-08-14T00:00:00Z".to_string()),
            completed_at: None,
        }
    }

    fn node(session_id: Option<&str>, submitted: bool) -> WorkflowNodeRow {
        WorkflowNodeRow {
            node_id: "node_test".to_string(),
            workflow_id: "wf_test".to_string(),
            parent_node_id: None,
            node_type: "agent".to_string(),
            phase: "Test".to_string(),
            title: "Test".to_string(),
            instructions: String::new(),
            inputs: "[]".to_string(),
            output: "out.md".to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
            introduced_revision: 1,
            retired_revision: None,
            session_id: session_id.map(str::to_string),
            submitted_at: submitted.then(|| "2026-08-14T00:00:30Z".to_string()),
            submitted_runtime_instance_id: submitted.then(|| "rtinst_test".to_string()),
            exit_request_started_at: None,
            created_at: "2026-08-14T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn derives_agent_status_from_workflow_facts_and_session_projection() {
        assert_eq!(
            derive_status(&workflow("running"), &node(None, false), None, false, true),
            WorkflowAgentStatus::Pending
        );
        assert_eq!(
            derive_status(
                &workflow("running"),
                &node(Some("s"), false),
                Some("starting"),
                false,
                true
            ),
            WorkflowAgentStatus::Starting
        );
        assert_eq!(
            derive_status(
                &workflow("running"),
                &node(Some("s"), false),
                Some("busy"),
                false,
                true
            ),
            WorkflowAgentStatus::Running
        );
        assert_eq!(
            derive_status(
                &workflow("paused"),
                &node(Some("s"), false),
                Some("interrupted"),
                false,
                true
            ),
            WorkflowAgentStatus::Paused
        );
        assert_eq!(
            derive_status(
                &workflow("paused"),
                &node(Some("s"), true),
                Some("interrupted"),
                false,
                true
            ),
            WorkflowAgentStatus::Paused
        );
        assert_eq!(
            derive_status(
                &workflow("idle"),
                &node(Some("s"), false),
                Some("idle"),
                false,
                true
            ),
            WorkflowAgentStatus::Idle
        );
        assert_eq!(
            derive_status(
                &workflow("running"),
                &node(Some("s"), true),
                Some("idle"),
                false,
                true
            ),
            WorkflowAgentStatus::Exiting
        );
        assert_eq!(
            derive_status(
                &workflow("running"),
                &node(Some("s"), true),
                Some("exited"),
                false,
                true
            ),
            WorkflowAgentStatus::Submitted
        );
        assert_eq!(
            derive_status(&workflow("failed"), &node(None, false), None, true, true),
            WorkflowAgentStatus::Failed
        );
        assert_eq!(
            derive_status(
                &workflow("running"),
                &node(Some("missing"), false),
                None,
                false,
                true
            ),
            WorkflowAgentStatus::Unknown
        );
    }

    #[test]
    fn missing_bound_session_remains_unknown_even_at_failure_location() {
        assert_eq!(
            derive_status(
                &workflow("failed"),
                &node(Some("missing"), false),
                None,
                true,
                true
            ),
            WorkflowAgentStatus::Unknown,
        );
    }
}
