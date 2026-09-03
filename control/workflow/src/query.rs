use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path},
};

use pontia_core::time::utc_now;
use pontia_storage_sqlite::{
    models::workflows::{WorkflowNodeRow, WorkflowRow},
    repositories::{
        sessions::SqliteSessionRepository, turns::SqliteTurnRepository,
        workflows::SqliteWorkflowRepository,
    },
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
    pub definition_file: String,
    pub active_patch: Option<WorkflowActivePatchView>,
    pub current_node: Option<WorkflowNodeContextView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowActivePatchView {
    pub patch_id: String,
    pub state: String,
    pub base_revision: i64,
    pub request_document_ref: String,
    pub requesting_node_id: String,
    pub requesting_session_id: String,
    pub requesting_turn_id: String,
    pub replanner_session_id: Option<String>,
    pub replanner_turn_id: Option<String>,
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
    pub session_id: Option<String>,
    pub turn_ids: Vec<String>,
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
pub struct WorkflowPatchHistoryView {
    pub patch_id: String,
    pub state: String,
    pub outcome: Option<String>,
    pub base_revision: i64,
    pub result_revision: Option<i64>,
    pub requesting_node_id: String,
    pub requesting_session_id: String,
    pub requesting_turn_id: String,
    pub requesting_runtime_instance_id: String,
    pub replanner_session_id: Option<String>,
    pub replanner_turn_id: Option<String>,
    pub replanner_runtime_instance_id: Option<String>,
    pub added_node_ids: Vec<String>,
    pub retired_node_ids: Vec<String>,
    pub request_document_ref: String,
    pub decision_document_ref: Option<String>,
    pub reason_document_ref: Option<String>,
    pub blocked_draft_ref: Option<String>,
    pub requested_at: String,
    pub planning_at: Option<String>,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTimelineView {
    pub workflow_id: String,
    pub entries: Vec<WorkflowTimelineEntryView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowTimelineEntryView {
    pub fact_kind: String,
    pub source: String,
    pub event_id: String,
    pub event_type: String,
    pub persisted_at: String,
    pub occurred_at: Option<String>,
    pub workflow_sequence: Option<i64>,
    pub agent_event_order: Option<i64>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub node_id: Option<String>,
    pub patch_ids: Vec<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowDocumentView {
    pub workflow_id: String,
    pub document_ref: String,
    pub content: String,
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
    turns: SqliteTurnRepository,
}

impl WorkflowQueryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            workflows: SqliteWorkflowRepository::new(pool.clone()),
            sessions: SqliteSessionRepository::new(pool.clone()),
            turns: SqliteTurnRepository::new(pool),
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

    pub async fn get_workflow_snapshot(
        &self,
        workflow_id: &str,
        pontia_home: &Path,
    ) -> Result<Option<WorkflowDetailView>> {
        let Some(mut workflow) = self.get_workflow(workflow_id).await? else {
            return Ok(None);
        };
        workflow.definition_file = Some(definition_file(pontia_home, workflow_id));
        workflow.active_patch = self.active_patch(workflow_id).await?;
        Ok(Some(workflow))
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
            let turn_ids = match node.session_id.as_deref() {
                Some(session_id) => self
                    .turns
                    .list_turns(session_id)
                    .await?
                    .into_iter()
                    .map(|turn| turn.turn_id)
                    .collect(),
                None => Vec::new(),
            };
            views.push(WorkflowGraphNodeView {
                node_id: node.node_id,
                parent_node_id: node.parent_node_id,
                node_type: node.node_type,
                session_id: node.session_id,
                turn_ids,
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
        let Some(workflow) = self.get_workflow_snapshot(workflow_id, pontia_home).await? else {
            return Ok(None);
        };
        let current_node = match workflow.current_node_id.as_deref() {
            Some(current_node_id) => {
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
                Some(WorkflowNodeContextView {
                    instructions: node.instructions,
                    inputs,
                    output: node.output,
                })
            }
            None => None,
        };
        let definition_file = definition_file(pontia_home, workflow_id);
        let active_patch = workflow.active_patch.clone();
        Ok(Some(WorkflowContextView {
            workflow,
            definition_file,
            active_patch,
            current_node,
        }))
    }

    pub async fn list_workflow_patches(
        &self,
        workflow_id: &str,
    ) -> Result<Option<Vec<WorkflowPatchHistoryView>>> {
        if self.workflows.get_workflow(workflow_id).await?.is_none() {
            return Ok(None);
        }
        let nodes = self.workflows.list_node_history(workflow_id).await?;
        let patches = self.workflows.list_patches(workflow_id).await?;
        Ok(Some(
            patches
                .into_iter()
                .map(|patch| {
                    let changed_revision = patch
                        .result_revision
                        .filter(|revision| *revision > patch.base_revision);
                    let added_node_ids = changed_revision
                        .map(|revision| {
                            nodes
                                .iter()
                                .filter(|node| node.introduced_revision == revision)
                                .map(|node| node.node_id.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    let retired_node_ids = changed_revision
                        .map(|revision| {
                            nodes
                                .iter()
                                .filter(|node| node.retired_revision == Some(revision))
                                .map(|node| node.node_id.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    WorkflowPatchHistoryView {
                        patch_id: patch.patch_id,
                        outcome: matches!(patch.state.as_str(), "applied" | "rejected" | "blocked")
                            .then(|| patch.state.clone()),
                        state: patch.state,
                        base_revision: patch.base_revision,
                        result_revision: patch.result_revision,
                        requesting_node_id: patch.requesting_node_id,
                        requesting_session_id: patch.requesting_session_id,
                        requesting_turn_id: patch.requesting_turn_id,
                        requesting_runtime_instance_id: patch.requesting_runtime_instance_id,
                        replanner_session_id: patch.replanner_session_id,
                        replanner_turn_id: patch.replanner_turn_id,
                        replanner_runtime_instance_id: patch.replanner_runtime_instance_id,
                        added_node_ids,
                        retired_node_ids,
                        request_document_ref: patch.request_document_ref,
                        decision_document_ref: patch.decision_document_ref,
                        reason_document_ref: patch.reason_document_ref,
                        blocked_draft_ref: patch.blocked_draft_ref,
                        requested_at: patch.requested_at,
                        planning_at: patch.planning_at,
                        resolved_at: patch.resolved_at,
                    }
                })
                .collect(),
        ))
    }

    pub async fn get_workflow_timeline(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowTimelineView>> {
        if self.workflows.get_workflow(workflow_id).await?.is_none() {
            return Ok(None);
        }
        let nodes = self.workflows.list_node_history(workflow_id).await?;
        let patches = self.workflows.list_patches(workflow_id).await?;
        let node_by_session: HashMap<&str, &str> = nodes
            .iter()
            .filter_map(|node| {
                node.session_id
                    .as_deref()
                    .map(|session_id| (session_id, node.node_id.as_str()))
            })
            .collect();
        let workflow_events = self.workflows.list_events(workflow_id).await?;
        let agent_events = self
            .workflows
            .list_workflow_agent_events(workflow_id)
            .await?;
        let mut entries = Vec::with_capacity(workflow_events.len() + agent_events.len());

        for event in workflow_events {
            let payload: serde_json::Value = serde_json::from_str(&event.payload)?;
            let patch_ids = payload
                .get("patch_id")
                .and_then(serde_json::Value::as_str)
                .map(|id| vec![id.to_string()])
                .unwrap_or_default();
            let node_id = payload
                .get("node_id")
                .or_else(|| payload.get("requesting_node_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let session_id = payload
                .get("requesting_session_id")
                .or_else(|| payload.get("replanner_session_id"))
                .or_else(|| payload.get("session_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let turn_id = payload
                .get("requesting_turn_id")
                .or_else(|| payload.get("replanner_turn_id"))
                .or_else(|| payload.get("turn_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            entries.push(WorkflowTimelineEntryView {
                fact_kind: "pontia_orchestration".to_string(),
                source: "pontia_workflow".to_string(),
                event_id: event.event_id,
                event_type: event.event_type,
                persisted_at: event.created_at,
                occurred_at: None,
                workflow_sequence: Some(event.sequence),
                agent_event_order: None,
                session_id,
                turn_id,
                node_id,
                patch_ids,
                payload,
            });
        }
        for event in agent_events {
            let patch_ids = patches
                .iter()
                .filter(|patch| match event.turn_id.as_deref() {
                    Some(turn_id) => {
                        patch.requesting_turn_id == turn_id
                            || patch.replanner_turn_id.as_deref() == Some(turn_id)
                    }
                    None => {
                        patch.requesting_session_id == event.session_id
                            || patch.replanner_session_id.as_deref()
                                == Some(event.session_id.as_str())
                    }
                })
                .map(|patch| patch.patch_id.clone())
                .collect();
            let fact_kind = if matches!(
                event.source.as_str(),
                "agent_client" | "agent_adapter" | "runtime_manager"
            ) {
                "agent_lifecycle"
            } else {
                "pontia_orchestration"
            };
            entries.push(WorkflowTimelineEntryView {
                fact_kind: fact_kind.to_string(),
                source: event.source,
                event_id: event.event_id,
                event_type: event.event_type,
                persisted_at: event.created_at,
                occurred_at: Some(event.occurred_at),
                workflow_sequence: None,
                agent_event_order: Some(event.rowid),
                node_id: node_by_session
                    .get(event.session_id.as_str())
                    .map(|id| (*id).to_string()),
                session_id: Some(event.session_id),
                turn_id: event.turn_id,
                patch_ids,
                payload: serde_json::from_str(&event.payload)?,
            });
        }
        entries.sort_by(|left, right| {
            let left_source = (left.fact_kind == "agent_lifecycle") as u8;
            let right_source = (right.fact_kind == "agent_lifecycle") as u8;
            (&left.persisted_at, left_source, &left.event_id).cmp(&(
                &right.persisted_at,
                right_source,
                &right.event_id,
            ))
        });
        Ok(Some(WorkflowTimelineView {
            workflow_id: workflow_id.to_string(),
            entries,
        }))
    }

    pub async fn read_workflow_document(
        &self,
        workflow_id: &str,
        document_ref: &str,
        pontia_home: &Path,
    ) -> Result<Option<WorkflowDocumentView>> {
        if self.workflows.get_workflow(workflow_id).await?.is_none() {
            return Ok(None);
        }
        let safe_ref = Path::new(document_ref);
        if document_ref.is_empty()
            || safe_ref.is_absolute()
            || safe_ref
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(pontia_core::Error::Domain(
                "invalid Workflow document reference".to_string(),
            )
            .into());
        }
        let patches = self.workflows.list_patches(workflow_id).await?;
        let authorized = document_ref == "workflow.toml"
            || patches.iter().any(|patch| {
                patch.request_document_ref == document_ref
                    || patch.decision_document_ref.as_deref() == Some(document_ref)
                    || patch.reason_document_ref.as_deref() == Some(document_ref)
                    || patch.blocked_draft_ref.as_deref() == Some(document_ref)
            });
        if !authorized {
            return Err(pontia_core::Error::NotFound(format!(
                "Workflow document {document_ref} not found"
            ))
            .into());
        }
        let workflow_dir = pontia_home.join("workflows").join(workflow_id);
        let canonical_dir = tokio::fs::canonicalize(&workflow_dir)
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Error::Pontia(pontia_core::Error::NotFound(format!(
                        "Workflow document {document_ref} not found"
                    )))
                } else {
                    error.into()
                }
            })?;
        let path = workflow_dir.join(safe_ref);
        let canonical_path = tokio::fs::canonicalize(&path).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::Pontia(pontia_core::Error::NotFound(format!(
                    "Workflow document {document_ref} not found"
                )))
            } else {
                error.into()
            }
        })?;
        if !canonical_path.starts_with(canonical_dir) {
            return Err(pontia_core::Error::NotFound(format!(
                "Workflow document {document_ref} not found"
            ))
            .into());
        }
        let content = tokio::fs::read_to_string(canonical_path)
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::InvalidData {
                    Error::Pontia(pontia_core::Error::Domain(format!(
                        "Workflow document {document_ref} is not UTF-8"
                    )))
                } else {
                    error.into()
                }
            })?;
        Ok(Some(WorkflowDocumentView {
            workflow_id: workflow_id.to_string(),
            document_ref: document_ref.to_string(),
            content,
        }))
    }

    async fn active_patch(&self, workflow_id: &str) -> Result<Option<WorkflowActivePatchView>> {
        Ok(self
            .workflows
            .get_active_patch(workflow_id)
            .await?
            .map(|patch| WorkflowActivePatchView {
                patch_id: patch.patch_id,
                state: patch.state,
                base_revision: patch.base_revision,
                request_document_ref: patch.request_document_ref,
                requesting_node_id: patch.requesting_node_id,
                requesting_session_id: patch.requesting_session_id,
                requesting_turn_id: patch.requesting_turn_id,
                replanner_session_id: patch.replanner_session_id,
                replanner_turn_id: patch.replanner_turn_id,
            }))
    }
}

fn definition_file(pontia_home: &Path, workflow_id: &str) -> String {
    pontia_home
        .join("workflows")
        .join(workflow_id)
        .join("workflow.toml")
        .display()
        .to_string()
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
