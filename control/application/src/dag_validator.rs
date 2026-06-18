use std::collections::{HashMap, HashSet};

use super::*;

pub(crate) const VALID_KINDS: &[&str] = &[
    "design",
    "implementation",
    "review",
    "test",
    "debug",
    "documentation",
    "planning",
    "other",
];
pub(crate) const VALID_ACTIONS: &[&str] = &["agent_turn", "human_input", "noop"];

pub(crate) fn validate_plan_shape(payload: &SubmitPlanPayload) -> Result<()> {
    if payload.mode != "initial_dag" {
        return Err(Error::Domain(format!(
            "unsupported initial DAG mode: {}",
            payload.mode
        )));
    }
    if payload.work_items.is_empty() {
        return Err(Error::Domain(
            "initial DAG must include work items".to_string(),
        ));
    }
    validate_work_item_drafts(&payload.work_items)?;

    let ids = temp_id_set(&payload.work_items)?;
    for edge in &payload.edges {
        validate_edge_type(&edge.edge_type)?;
        if !ids.contains(&edge.from_work_item_id) {
            return Err(Error::Domain(format!(
                "edge references unknown from work item {}",
                edge.from_work_item_id
            )));
        }
        if !ids.contains(&edge.to_work_item_id) {
            return Err(Error::Domain(format!(
                "edge references unknown to work item {}",
                edge.to_work_item_id
            )));
        }
    }
    validate_acyclic(ids.iter().cloned(), &payload.edges)
}

pub(crate) fn validate_work_item_drafts(work_items: &[WorkItemDraft]) -> Result<()> {
    for work_item in work_items {
        if work_item.title.trim().is_empty() {
            return Err(Error::Domain("work item title is required".to_string()));
        }
        if work_item.description.trim().is_empty() {
            return Err(Error::Domain(
                "work item description is required".to_string(),
            ));
        }
        if !VALID_KINDS.contains(&work_item.kind.as_str()) {
            return Err(Error::Domain(format!(
                "unsupported work item kind: {}",
                work_item.kind
            )));
        }
        if !VALID_ACTIONS.contains(&work_item.action.as_str()) {
            return Err(Error::Domain(format!(
                "unsupported work item action: {}",
                work_item.action
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_edge_type(edge_type: &str) -> Result<()> {
    if edge_type == "depends_on" {
        Ok(())
    } else {
        Err(Error::Domain(format!("unsupported edge type: {edge_type}")))
    }
}

pub(crate) fn temp_id_set(work_items: &[WorkItemDraft]) -> Result<HashSet<String>> {
    let mut ids = HashSet::new();
    for work_item in work_items {
        let Some(temp_id) = work_item.temp_id.as_ref() else {
            return Err(Error::Domain(
                "initial DAG work items must include temp_id".to_string(),
            ));
        };
        if temp_id.trim().is_empty() {
            return Err(Error::Domain("work item temp_id is required".to_string()));
        }
        if !ids.insert(temp_id.clone()) {
            return Err(Error::Domain(format!(
                "duplicate work item temp_id: {temp_id}"
            )));
        }
    }
    Ok(ids)
}

pub(crate) fn validate_acyclic<I>(nodes: I, edges: &[WorkItemEdgeDraft]) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        adjacency.entry(node).or_default();
    }
    for edge in edges {
        adjacency
            .entry(edge.from_work_item_id.clone())
            .or_default()
            .push(edge.to_work_item_id.clone());
        adjacency.entry(edge.to_work_item_id.clone()).or_default();
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for node in adjacency.keys() {
        if has_cycle(node, &adjacency, &mut visiting, &mut visited) {
            return Err(Error::Domain("DAG contains a cycle".to_string()));
        }
    }
    Ok(())
}

fn has_cycle(
    node: &str,
    adjacency: &HashMap<String, Vec<String>>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
) -> bool {
    if visited.contains(node) {
        return false;
    }
    if !visiting.insert(node.to_string()) {
        return true;
    }
    if let Some(children) = adjacency.get(node) {
        for child in children {
            if has_cycle(child, adjacency, visiting, visited) {
                return true;
            }
        }
    }
    visiting.remove(node);
    visited.insert(node.to_string());
    false
}
