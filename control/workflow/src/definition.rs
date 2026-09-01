use serde::Serialize;

use crate::{Result, RunWorkflowRequest, WorkflowNodeDefinition};

#[derive(Serialize)]
struct WorkflowFile<'a> {
    title: &'a str,
    cwd: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    handoffs: Vec<WorkflowFileHandoff>,
    nodes: &'a [WorkflowNodeDefinition],
}

#[derive(Serialize)]
struct WorkflowFileHandoff {
    name: String,
    source: String,
}

pub(crate) fn render_workflow_file(request: &RunWorkflowRequest) -> Result<String> {
    let handoffs = request
        .handoffs
        .iter()
        .map(|handoff| WorkflowFileHandoff {
            name: handoff.name.clone(),
            source: format!("handoff/{}", handoff.name),
        })
        .collect();
    Ok(toml::to_string_pretty(&WorkflowFile {
        title: &request.title,
        cwd: &request.cwd,
        handoffs,
        nodes: &request.nodes,
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InitialHandoff, WorkflowNodeDefinition};

    #[test]
    fn renders_a_self_contained_workflow_file() {
        let rendered = render_workflow_file(&RunWorkflowRequest {
            workflow_id: "wf_render".to_string(),
            title: "Render workflow".to_string(),
            cwd: "/workspace/project".to_string(),
            handoffs: vec![InitialHandoff {
                name: "brief.md".to_string(),
                content: "Not embedded in the definition".to_string(),
            }],
            nodes: vec![WorkflowNodeDefinition {
                node_type: "agent".to_string(),
                phase: "Build".to_string(),
                title: "Implement".to_string(),
                instructions: "Implement it.".to_string(),
                inputs: vec!["brief.md".to_string()],
                output: "result.md".to_string(),
                execution_profile_id: None,
                execution_profile_version: None,
            }],
        })
        .expect("render workflow file");

        assert!(rendered.contains("cwd = \"/workspace/project\""));
        assert!(rendered.contains("source = \"handoff/brief.md\""));
        assert!(rendered.contains("type = \"agent\""));
        assert!(!rendered.contains("Not embedded in the definition"));
    }
}
