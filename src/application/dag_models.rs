use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkItemDraft {
    pub temp_id: Option<String>,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub action: String,
    pub execution_profile_id: String,
    pub execution_profile_version: Option<String>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub optional: bool,
    #[serde(default = "default_parallelizable")]
    pub parallelizable: bool,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default = "empty_object")]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemEdgeDraft {
    pub from_work_item_id: String,
    pub to_work_item_id: String,
    #[serde(default = "default_edge_type")]
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubmitPlanPayload {
    pub mode: String,
    pub summary: String,
    #[serde(default)]
    pub work_items: Vec<WorkItemDraft>,
    #[serde(default)]
    pub edges: Vec<WorkItemEdgeDraft>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub risks: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DagPatch {
    pub summary: String,
    #[serde(default)]
    pub base_revision: Option<i64>,
    #[serde(default)]
    pub anchor_work_item_id: Option<String>,
    #[serde(default = "default_supersede_policy")]
    pub supersede_policy: String,
    #[serde(default)]
    pub operations: Vec<PatchOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PatchOperation {
    AddWorkItem {
        work_item: WorkItemDraft,
    },
    AddEdge {
        edge: WorkItemEdgeDraft,
    },
    RemoveEdge {
        edge: WorkItemEdgeDraft,
    },
    ReplaceEdge {
        from: WorkItemEdgeDraft,
        to: WorkItemEdgeDraft,
    },
    SupersedeWorkItem {
        work_item_id: String,
        reason: String,
    },
    ReactivateWorkItem {
        work_item_id: String,
        reason: String,
    },
    SetWorkItemOutcome {
        work_item_id: String,
        outcome_state: String,
        reason: String,
    },
    InsertWorkItemBetween {
        from_work_item_id: String,
        to_work_item_id: String,
        work_item: WorkItemDraft,
    },
    ReplaceDownstream {
        anchor_work_item_id: String,
        old_work_item_ids: Vec<String>,
        replacement: WorkItemDraft,
        #[serde(default = "default_true")]
        supersede_old: bool,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DagPatchApplySummary {
    pub anchor_work_item_id: Option<String>,
    pub supersede_policy: String,
    pub superseded_work_item_ids: Vec<String>,
    pub added_work_item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DagProposal {
    pub proposal_id: String,
    pub task_id: String,
    pub mode: String,
    pub state: String,
    pub summary: String,
    pub proposal_json: Value,
    pub validation_json: Value,
    pub created_by_session_id: Option<String>,
    pub revision: i64,
    pub supersedes_proposal_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkItemRecord {
    pub work_item_id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub action: String,
    pub execution_profile_id: String,
    pub execution_profile_version: Option<String>,
    pub active: bool,
    pub priority: i64,
    pub optional: bool,
    pub parallelizable: bool,
    pub acceptance_criteria: Value,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkItemRunRecord {
    pub run_id: String,
    pub work_item_id: String,
    pub task_id: String,
    pub attempt: i64,
    pub state: String,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub client_type: Option<String>,
    pub execution_profile_id: String,
    pub execution_profile_version: Option<String>,
    pub rendered_prompt_ref: Option<String>,
    pub output_summary: Option<String>,
    pub failure: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DagSignalRecord {
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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SubmitResultPayload {
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub outputs: Vec<Value>,
    #[serde(default)]
    pub failure: Option<Value>,
    #[serde(default)]
    pub signals: Vec<RaiseSignalPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RaiseSignalPayload {
    pub kind: String,
    pub summary: String,
    pub detail: Option<String>,
    #[serde(default = "default_severity")]
    pub severity: String,
    #[serde(default)]
    pub related_refs: Vec<Value>,
}

fn default_parallelizable() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_edge_type() -> String {
    "depends_on".to_string()
}

fn default_supersede_policy() -> String {
    "explicit_only".to_string()
}

fn default_severity() -> String {
    "medium".to_string()
}

fn empty_object() -> Value {
    json!({})
}
