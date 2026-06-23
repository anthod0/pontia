use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use pontia_config::GraphRuntimeConfig;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProvenanceNode {
    pub id: String,
    pub kind: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProvenanceEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskProvenance {
    pub nodes: Vec<ProvenanceNode>,
    pub edges: Vec<ProvenanceEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskNode {
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub ref_: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkItemNode {
    pub work_item_id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub action: String,
    pub execution_profile_id: String,
    pub execution_profile_version: Option<String>,
    pub review_policy: Option<Value>,
    pub execution_policy: Option<Value>,
    pub escalation_policy: Option<Value>,
    pub priority: i64,
    pub optional: bool,
    pub parallelizable: bool,
    pub acceptance_criteria: Value,
    pub active: bool,
    pub ref_: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraphEdgeKind {
    DependsOn,
    Reviews,
    Supersedes,
    CausedBy,
}

impl GraphEdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DependsOn => "depends_on",
            Self::Reviews => "reviews",
            Self::Supersedes => "supersedes",
            Self::CausedBy => "caused_by",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "depends_on" => Some(Self::DependsOn),
            "reviews" => Some(Self::Reviews),
            "supersedes" => Some(Self::Supersedes),
            "caused_by" => Some(Self::CausedBy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemEdgeRecord {
    pub edge_id: String,
    pub task_id: String,
    pub from_work_item_id: String,
    pub to_work_item_id: String,
    pub edge_type: GraphEdgeKind,
    pub ref_: Option<String>,
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignalNode {
    pub signal_id: String,
    pub task_id: String,
    pub work_item_id: Option<String>,
    pub run_id: Option<String>,
    pub source_session_id: Option<String>,
    pub source: String,
    pub kind: String,
    pub summary: String,
    pub detail: Option<String>,
    pub severity: String,
    pub related_refs: Value,
    pub state: String,
    pub ref_: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskGraphSnapshot {
    pub task: Option<TaskNode>,
    pub work_items: Vec<WorkItemNode>,
    pub edges: Vec<WorkItemEdgeRecord>,
    pub signals: Vec<SignalNode>,
}
