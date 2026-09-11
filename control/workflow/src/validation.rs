use std::{
    collections::HashMap,
    path::{Component, Path},
};

use crate::{Error, Result, RunWorkflowRequest};

pub(crate) fn validate_run_request(request: &RunWorkflowRequest) -> Result<()> {
    validate_workflow_id(&request.workflow_id)?;
    if request.title.trim().is_empty() {
        return Err(Error::InvalidDefinition(
            "title must not be empty".to_string(),
        ));
    }
    if request.nodes.is_empty() {
        return Err(Error::InvalidDefinition(
            "at least one Agent Node is required".to_string(),
        ));
    }

    let mut available_handoffs = HashMap::new();
    for handoff in &request.handoffs {
        validate_handoff_file_name(&handoff.name)?;
        if available_handoffs
            .insert(handoff.name.clone(), "an initial Handoff".to_string())
            .is_some()
        {
            return Err(Error::InvalidDefinition(format!(
                "duplicate initial Handoff file {}",
                handoff.name
            )));
        }
    }
    for node in &request.nodes {
        if node.node_type != "agent" {
            return Err(Error::UnsupportedNodeType(node.node_type.clone()));
        }
        if node.title.trim().is_empty() {
            return Err(Error::InvalidDefinition(
                "Agent Node title must not be empty".to_string(),
            ));
        }
        let phase = node.phase.trim();
        if phase.is_empty() {
            return Err(Error::InvalidDefinition(
                "Agent Node phase must not be empty".to_string(),
            ));
        }
        if phase.chars().count() > 80 {
            return Err(Error::InvalidDefinition(
                "Agent Node phase must be at most 80 characters".to_string(),
            ));
        }
        match (
            node.execution_profile_id.as_ref(),
            node.execution_profile_version.as_ref(),
        ) {
            (Some(_), Some(_)) | (None, None) => {}
            _ => {
                return Err(Error::InvalidDefinition(format!(
                    "Agent Node {} must specify both execution_profile_id and execution_profile_version",
                    node.title
                )));
            }
        }
        for input in &node.inputs {
            validate_handoff_file_name(input)?;
            if !available_handoffs.contains_key(input) {
                return Err(Error::InvalidDefinition(format!(
                    "Agent Node {} input {input} is not an initial Handoff or prior Agent Node output",
                    node.title
                )));
            }
        }
        validate_handoff_file_name(&node.output)?;
        if let Some(owner) = available_handoffs.get(&node.output) {
            return Err(Error::InvalidDefinition(format!(
                "Agent Node {} output {} conflicts with {owner}; every Agent Node must use a unique output name so each Handoff file has exactly one writer",
                node.title, node.output
            )));
        }
        available_handoffs.insert(node.output.clone(), format!("Agent Node {}", node.title));
    }
    Ok(())
}

fn validate_workflow_id(workflow_id: &str) -> Result<()> {
    let mut components = Path::new(workflow_id).components();
    if matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && workflow_id.starts_with("wf_")
    {
        return Ok(());
    }
    Err(Error::InvalidWorkflowId(workflow_id.to_string()))
}

pub(crate) fn validate_handoff_file_name(name: &str) -> Result<()> {
    let mut components = Path::new(name).components();
    if matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none() {
        return Ok(());
    }
    Err(Error::InvalidHandoffFileName(name.to_string()))
}

pub(crate) fn is_runtime_control_unavailable(error: &Error) -> bool {
    matches!(error, Error::RuntimeControlUnavailable { .. })
}
