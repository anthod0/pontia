use std::collections::{HashMap, HashSet};

use crate::models::WorkflowNodeRow;
use crate::{Error, Result};

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
