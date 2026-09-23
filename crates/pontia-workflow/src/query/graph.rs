use std::collections::{HashMap, HashSet};

use pontia_storage_sqlite::models::workflows::WorkflowNodeRow;
use serde::Serialize;

use super::WorkflowQueryService;
use crate::{Error, Result};

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

impl WorkflowQueryService {
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
}

pub(super) fn ordered_nodes(
    workflow_id: &str,
    nodes: Vec<WorkflowNodeRow>,
) -> Result<Vec<WorkflowNodeRow>> {
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
