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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkflowNodeDefinition {
    #[serde(rename = "type")]
    pub node_type: String,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    pub inputs: Vec<String>,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_profile_version: Option<String>,
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
