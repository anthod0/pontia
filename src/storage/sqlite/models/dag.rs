#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkItemRow {
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
    pub acceptance_criteria: String,
    pub metadata: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkItemEdgeRow {
    pub edge_id: String,
    pub task_id: String,
    pub from_work_item_id: String,
    pub to_work_item_id: String,
    pub edge_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkItemRunRow {
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
    pub failure: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkItemRuntimeProjectionRow {
    pub work_item_id: String,
    pub current_run_id: Option<String>,
    pub current_state: String,
    pub current_attempt: i64,
    pub ready_at: Option<String>,
    pub blocked_reason: Option<String>,
    pub outcome_state: Option<String>,
    pub outcome_reason: Option<String>,
    pub replanned_from_state: Option<String>,
    pub retry_count: i64,
    pub max_retries: i64,
    pub priority: i64,
    pub optional: bool,
    pub parallelizable: bool,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DagProposalRow {
    pub proposal_id: String,
    pub task_id: String,
    pub mode: String,
    pub state: String,
    pub summary: String,
    pub proposal_json: String,
    pub validation_json: String,
    pub created_by_session_id: Option<String>,
    pub created_by_turn_id: String,
    pub revision: i64,
    pub supersedes_proposal_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DagSignalRow {
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
    pub related_refs: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}
