use serde_json::{Value, json};

use super::{GraphEdgeKind, SignalNode, TaskNode, WorkItemNode};

#[derive(Debug, Clone, PartialEq)]
pub struct UpsertTaskRequest {
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub ref_: Option<String>,
    pub metadata: Value,
}

impl From<TaskNode> for UpsertTaskRequest {
    fn from(node: TaskNode) -> Self {
        Self {
            task_id: node.task_id,
            title: node.title,
            description: node.description,
            ref_: node.ref_,
            metadata: node.metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpsertWorkItemRequest {
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
}

impl From<WorkItemNode> for UpsertWorkItemRequest {
    fn from(node: WorkItemNode) -> Self {
        Self {
            work_item_id: node.work_item_id,
            task_id: node.task_id,
            title: node.title,
            description: node.description,
            kind: node.kind,
            action: node.action,
            execution_profile_id: node.execution_profile_id,
            execution_profile_version: node.execution_profile_version,
            review_policy: node.review_policy,
            execution_policy: node.execution_policy,
            escalation_policy: node.escalation_policy,
            priority: node.priority,
            optional: node.optional,
            parallelizable: node.parallelizable,
            acceptance_criteria: node.acceptance_criteria,
            active: node.active,
            ref_: node.ref_,
            metadata: node.metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddWorkItemEdgeRequest {
    pub task_id: String,
    pub from_work_item_id: String,
    pub to_work_item_id: String,
    pub edge_type: GraphEdgeKind,
    pub ref_: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpsertSignalRequest {
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
}

impl From<SignalNode> for UpsertSignalRequest {
    fn from(node: SignalNode) -> Self {
        Self {
            signal_id: node.signal_id,
            task_id: node.task_id,
            work_item_id: node.work_item_id,
            run_id: node.run_id,
            source_session_id: node.source_session_id,
            source: node.source,
            kind: node.kind,
            summary: node.summary,
            detail: node.detail,
            severity: node.severity,
            related_refs: node.related_refs,
            state: node.state,
            ref_: node.ref_,
            metadata: node.metadata,
        }
    }
}

impl Default for UpsertSignalRequest {
    fn default() -> Self {
        Self {
            signal_id: String::new(),
            task_id: String::new(),
            work_item_id: None,
            run_id: None,
            source_session_id: None,
            source: "system".to_string(),
            kind: String::new(),
            summary: String::new(),
            detail: None,
            severity: "medium".to_string(),
            related_refs: json!([]),
            state: "open".to_string(),
            ref_: None,
            metadata: json!({}),
        }
    }
}
