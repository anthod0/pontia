use std::collections::{HashMap, HashSet};

use pontia_storage_sqlite::models::workflows::{WorkflowNodeRow, WorkflowRow};
use serde::{Deserialize, Serialize};

use crate::{
    AcceptedWorkflowDefinition, AcceptedWorkflowNode, DefinitionChangePlan, Error, InitialHandoff,
    PlannedNodeParent, PlannedWorkflowNode, Result, RunWorkflowRequest, WorkflowDefinitionHandoff,
    WorkflowNodeDefinition, validation::validate_run_request,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkflowFile {
    workflow_id: String,
    revision: i64,
    title: String,
    cwd: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    handoffs: Vec<WorkflowFileHandoff>,
    nodes: Vec<WorkflowFileNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkflowFileHandoff {
    name: String,
    source: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkflowFileNode {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(flatten)]
    definition: WorkflowNodeDefinition,
}

pub fn render_accepted_workflow_definition(
    definition: &AcceptedWorkflowDefinition,
) -> Result<String> {
    let file = WorkflowFile {
        workflow_id: definition.workflow_id.clone(),
        revision: definition.revision,
        title: definition.title.clone(),
        cwd: definition.cwd.clone(),
        handoffs: definition
            .handoffs
            .iter()
            .map(|handoff| WorkflowFileHandoff {
                name: handoff.name.clone(),
                source: handoff.source.clone(),
            })
            .collect(),
        nodes: definition
            .nodes
            .iter()
            .map(|node| WorkflowFileNode {
                id: Some(node.node_id.clone()),
                definition: node.definition.clone(),
            })
            .collect(),
    };
    Ok(toml::to_string_pretty(&file)?)
}

pub fn plan_workflow_definition_change(
    accepted: &AcceptedWorkflowDefinition,
    candidate_bytes: &[u8],
) -> Result<DefinitionChangePlan> {
    let source = std::str::from_utf8(candidate_bytes).map_err(|_| {
        Error::InvalidDefinition("candidate Workflow definition must be valid UTF-8".to_string())
    })?;
    let mut candidate: WorkflowFile = toml::from_str(source)
        .map_err(|error| Error::InvalidDefinition(format!("invalid candidate TOML: {error}")))?;
    normalize_file(&mut candidate);
    validate_candidate_graph(&candidate)?;
    validate_candidate_metadata(accepted, &candidate)?;

    let accepted_by_id = accepted
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let retired_ids = accepted
        .retired_node_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let protected_ids = accepted
        .nodes
        .iter()
        .filter(|node| node.activated)
        .map(|node| node.node_id.as_str())
        .collect::<HashSet<_>>();
    let mut seen_ids = HashSet::new();
    let mut retained_node_ids = Vec::new();
    let mut introduced_nodes = Vec::new();
    let mut previous_parent = None;

    for (index, node) in candidate.nodes.iter().enumerate() {
        match node.id.as_deref() {
            Some(node_id) => {
                if retired_ids.contains(node_id) {
                    return Err(Error::InvalidDefinition(format!(
                        "retired Workflow Node identity {node_id} cannot be reintroduced"
                    )));
                }
                let Some(base_node) = accepted_by_id.get(node_id) else {
                    return Err(Error::InvalidDefinition(format!(
                        "candidate cannot introduce caller-provided Workflow Node identity {node_id}"
                    )));
                };
                if !seen_ids.insert(node_id) {
                    return Err(Error::InvalidDefinition(format!(
                        "duplicate Workflow Node identity {node_id}"
                    )));
                }
                if base_node.definition != node.definition {
                    return Err(Error::InvalidDefinition(format!(
                        "retained Workflow Node {node_id} definition cannot be changed; omit its identity to replace it"
                    )));
                }
                let candidate_parent = match &previous_parent {
                    None => None,
                    Some(PlannedNodeParent::Retained(parent_id)) => Some(parent_id.as_str()),
                    Some(PlannedNodeParent::Introduced(_)) => {
                        return Err(Error::InvalidDefinition(format!(
                            "retained Workflow Node {node_id} has a changed immutable parent; omit its identity to replace it"
                        )));
                    }
                };
                if base_node.parent_node_id.as_deref() != candidate_parent {
                    return Err(Error::InvalidDefinition(format!(
                        "retained Workflow Node {node_id} has a changed immutable parent; omit its identity to replace it"
                    )));
                }
                retained_node_ids.push(node_id.to_string());
                previous_parent = Some(PlannedNodeParent::Retained(node_id.to_string()));
            }
            None => {
                if accepted
                    .nodes
                    .get(index)
                    .is_some_and(|base_node| protected_ids.contains(base_node.node_id.as_str()))
                {
                    return Err(Error::InvalidDefinition(format!(
                        "protected Workflow Node {} cannot be replaced",
                        accepted.nodes[index].node_id
                    )));
                }
                let introduced_index = introduced_nodes.len();
                introduced_nodes.push(PlannedWorkflowNode {
                    candidate_index: index,
                    parent: previous_parent.clone(),
                    definition: node.definition.clone(),
                });
                previous_parent = Some(PlannedNodeParent::Introduced(introduced_index));
            }
        }
    }

    for (index, protected_node) in accepted
        .nodes
        .iter()
        .take_while(|node| node.activated)
        .enumerate()
    {
        if candidate
            .nodes
            .get(index)
            .and_then(|node| node.id.as_deref())
            != Some(protected_node.node_id.as_str())
        {
            return Err(Error::InvalidDefinition(format!(
                "protected Workflow Node {} and its position cannot be changed",
                protected_node.node_id
            )));
        }
    }

    let retained = retained_node_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let retired_node_ids = accepted
        .nodes
        .iter()
        .filter(|node| !retained.contains(node.node_id.as_str()))
        .map(|node| node.node_id.clone())
        .collect::<Vec<_>>();

    if retired_node_ids.is_empty() && introduced_nodes.is_empty() {
        Ok(DefinitionChangePlan::NoChange)
    } else {
        Ok(DefinitionChangePlan::Changed {
            retained_node_ids,
            retired_node_ids,
            introduced_nodes,
        })
    }
}

fn normalize_file(file: &mut WorkflowFile) {
    file.handoffs
        .sort_by(|left, right| (&left.name, &left.source).cmp(&(&right.name, &right.source)));
    for node in &mut file.nodes {
        node.definition.phase = node.definition.phase.trim().to_string();
        node.definition.inputs.sort();
    }
}

fn validate_candidate_metadata(
    accepted: &AcceptedWorkflowDefinition,
    candidate: &WorkflowFile,
) -> Result<()> {
    if candidate.workflow_id != accepted.workflow_id {
        return Err(Error::InvalidDefinition(
            "Workflow identity cannot be changed".to_string(),
        ));
    }
    if candidate.revision != accepted.revision {
        return Err(Error::InvalidDefinition(format!(
            "candidate revision {} does not match current revision {}",
            candidate.revision, accepted.revision
        )));
    }
    if candidate.title != accepted.title {
        return Err(Error::InvalidDefinition(
            "Workflow title cannot be changed".to_string(),
        ));
    }
    if candidate.cwd != accepted.cwd {
        return Err(Error::InvalidDefinition(
            "Workflow launch directory cannot be changed".to_string(),
        ));
    }
    let mut expected_handoffs = accepted
        .handoffs
        .iter()
        .map(|handoff| WorkflowFileHandoff {
            name: handoff.name.clone(),
            source: handoff.source.clone(),
        })
        .collect::<Vec<_>>();
    expected_handoffs
        .sort_by(|left, right| (&left.name, &left.source).cmp(&(&right.name, &right.source)));
    if candidate.handoffs != expected_handoffs {
        return Err(Error::InvalidDefinition(
            "initial Workflow Handoffs cannot be changed".to_string(),
        ));
    }
    Ok(())
}

fn validate_candidate_graph(candidate: &WorkflowFile) -> Result<()> {
    validate_run_request(&RunWorkflowRequest {
        workflow_id: candidate.workflow_id.clone(),
        title: candidate.title.clone(),
        cwd: candidate.cwd.clone(),
        handoffs: candidate
            .handoffs
            .iter()
            .map(|handoff| InitialHandoff {
                name: handoff.name.clone(),
                content: String::new(),
            })
            .collect(),
        nodes: candidate
            .nodes
            .iter()
            .map(|node| node.definition.clone())
            .collect(),
    })
}

pub(crate) fn accepted_definition_from_snapshot(
    workflow: &WorkflowRow,
    active_nodes: Vec<WorkflowNodeRow>,
    node_history: &[WorkflowNodeRow],
    snapshot_bytes: &[u8],
) -> Result<AcceptedWorkflowDefinition> {
    let source = std::str::from_utf8(snapshot_bytes).map_err(|_| {
        Error::InvalidDefinition("accepted Workflow definition must be valid UTF-8".to_string())
    })?;
    let mut snapshot: WorkflowFile = toml::from_str(source).map_err(|error| {
        Error::InvalidDefinition(format!("invalid accepted Workflow definition: {error}"))
    })?;
    normalize_file(&mut snapshot);
    if snapshot.workflow_id != workflow.workflow_id
        || snapshot.revision != workflow.current_revision
        || snapshot.title != workflow.title
        || snapshot.cwd != workflow.cwd
    {
        return Err(Error::InvalidDefinition(
            "accepted Workflow definition metadata does not match durable state".to_string(),
        ));
    }

    let mut remaining = active_nodes;
    let mut ordered = Vec::with_capacity(remaining.len());
    let mut parent: Option<String> = None;
    while !remaining.is_empty() {
        let matches = remaining
            .iter()
            .enumerate()
            .filter(|(_, node)| node.parent_node_id == parent)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(Error::InvalidDefinition(
                "accepted Workflow graph is not one linear chain".to_string(),
            ));
        }
        let node = remaining.remove(matches[0]);
        parent = Some(node.node_id.clone());
        ordered.push(AcceptedWorkflowNode {
            node_id: node.node_id,
            parent_node_id: node.parent_node_id,
            definition: WorkflowNodeDefinition {
                node_type: node.node_type,
                phase: node.phase,
                title: node.title,
                instructions: node.instructions,
                inputs: serde_json::from_str(&node.inputs)?,
                output: node.output,
                execution_profile_id: node.execution_profile_id,
                execution_profile_version: node.execution_profile_version,
            },
            activated: node.session_id.is_some(),
        });
    }

    let active_ids = ordered
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<HashSet<_>>();
    let snapshot_ids = snapshot
        .nodes
        .iter()
        .filter_map(|node| node.id.as_deref())
        .collect::<HashSet<_>>();
    if active_ids != snapshot_ids {
        return Err(Error::InvalidDefinition(
            "accepted Workflow definition graph does not match durable state".to_string(),
        ));
    }

    Ok(AcceptedWorkflowDefinition {
        workflow_id: workflow.workflow_id.clone(),
        revision: workflow.current_revision,
        title: workflow.title.clone(),
        cwd: workflow.cwd.clone(),
        handoffs: snapshot
            .handoffs
            .into_iter()
            .map(|handoff| WorkflowDefinitionHandoff {
                name: handoff.name,
                source: handoff.source,
            })
            .collect(),
        nodes: ordered,
        retired_node_ids: node_history
            .iter()
            .filter(|node| node.retired_revision.is_some())
            .map(|node| node.node_id.clone())
            .collect(),
    })
}

pub(crate) fn definition_handoffs(
    definition_bytes: &[u8],
) -> Result<Vec<WorkflowDefinitionHandoff>> {
    let source = std::str::from_utf8(definition_bytes).map_err(|_| {
        Error::InvalidDefinition("accepted Workflow definition must be valid UTF-8".to_string())
    })?;
    let mut definition: WorkflowFile = toml::from_str(source).map_err(|error| {
        Error::InvalidDefinition(format!("invalid accepted Workflow definition: {error}"))
    })?;
    normalize_file(&mut definition);
    Ok(definition
        .handoffs
        .into_iter()
        .map(|handoff| WorkflowDefinitionHandoff {
            name: handoff.name,
            source: handoff.source,
        })
        .collect())
}

pub(crate) fn accepted_definition_from_initial_request(
    request: &RunWorkflowRequest,
    nodes: Vec<AcceptedWorkflowNode>,
) -> AcceptedWorkflowDefinition {
    let mut handoffs = request
        .handoffs
        .iter()
        .map(|handoff| WorkflowDefinitionHandoff {
            name: handoff.name.clone(),
            source: format!("handoff/{}", handoff.name),
        })
        .collect::<Vec<_>>();
    handoffs.sort_by(|left, right| (&left.name, &left.source).cmp(&(&right.name, &right.source)));
    AcceptedWorkflowDefinition {
        workflow_id: request.workflow_id.clone(),
        revision: 1,
        title: request.title.clone(),
        cwd: request.cwd.clone(),
        handoffs,
        nodes,
        retired_node_ids: Vec::new(),
    }
}
