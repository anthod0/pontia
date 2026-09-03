#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunWorkflowRequest {
    pub workflow_id: String,
    pub title: String,
    pub cwd: String,
    pub handoffs: Vec<InitialHandoff>,
    pub nodes: Vec<WorkflowNodeDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialHandoff {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowNodeDefinition {
    #[serde(rename = "type")]
    pub node_type: String,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    #[serde(default)]
    pub inputs: Vec<String>,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_profile_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDefinitionHandoff {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedWorkflowNode {
    pub node_id: String,
    pub parent_node_id: Option<String>,
    pub definition: WorkflowNodeDefinition,
    pub activated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedWorkflowDefinition {
    pub workflow_id: String,
    pub revision: i64,
    pub title: String,
    pub cwd: String,
    pub handoffs: Vec<WorkflowDefinitionHandoff>,
    pub nodes: Vec<AcceptedWorkflowNode>,
    pub retired_node_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedNodeParent {
    Retained(String),
    Introduced(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWorkflowNode {
    pub candidate_index: usize,
    pub parent: Option<PlannedNodeParent>,
    pub definition: WorkflowNodeDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionChangePlan {
    NoChange,
    Changed {
        retained_node_ids: Vec<String>,
        retired_node_ids: Vec<String>,
        introduced_nodes: Vec<PlannedWorkflowNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunWorkflowOutcome {
    pub workflow_id: String,
    pub node_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitWorkflowNodeRequest {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub output: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartWorkflowOutcome {
    pub node_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestWorkflowPatch {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub document: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestWorkflowPatchOutcome {
    pub patch_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyWorkflowPatch {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub decision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyWorkflowPatchOutcome {
    pub patch_id: String,
    pub workflow_id: String,
    pub outcome: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockWorkflowPatch {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockWorkflowPatchOutcome {
    pub patch_id: String,
    pub workflow_id: String,
}
