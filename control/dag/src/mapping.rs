use super::*;
use pontia_storage_sqlite::models::dag::{
    DagProposalRow, DagSignalRow, WorkItemRunRow, WorkItemRuntimeProjectionRow,
};

pub(crate) fn work_item_node_to_record(node: WorkItemNode) -> WorkItemRecord {
    WorkItemRecord {
        work_item_id: node.work_item_id,
        task_id: node.task_id,
        title: node.title,
        description: node.description,
        kind: node.kind,
        action: node.action,
        execution_profile_id: node.execution_profile_id,
        execution_profile_version: node.execution_profile_version,
        active: node.active,
        priority: node.priority,
        optional: node.optional,
        parallelizable: node.parallelizable,
        acceptance_criteria: node.acceptance_criteria,
        metadata: node.metadata,
        created_at: node.created_at,
        updated_at: node.updated_at,
    }
}

pub(crate) fn graph_edge_record_to_view(edge: WorkItemEdgeRecord) -> WorkItemEdgeView {
    WorkItemEdgeView {
        edge_id: edge.edge_id,
        task_id: edge.task_id,
        from_work_item_id: edge.from_work_item_id,
        to_work_item_id: edge.to_work_item_id,
        edge_type: edge.edge_type.as_str().to_string(),
        created_at: edge.created_at,
    }
}

pub(crate) fn work_item_run_row_to_record(row: WorkItemRunRow) -> Result<WorkItemRunRecord> {
    Ok(WorkItemRunRecord {
        run_id: row.run_id,
        work_item_id: row.work_item_id,
        task_id: row.task_id,
        attempt: row.attempt,
        state: row.state,
        session_id: row.session_id,
        turn_id: row.turn_id,
        client_type: row.client_type,
        execution_profile_id: row.execution_profile_id,
        execution_profile_version: row.execution_profile_version,
        rendered_prompt_ref: row.rendered_prompt_ref,
        output_summary: row.output_summary,
        failure: row
            .failure
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        created_at: row.created_at,
        updated_at: row.updated_at,
        started_at: row.started_at,
        completed_at: row.completed_at,
    })
}

pub(crate) fn dag_signal_row_to_record(row: DagSignalRow) -> Result<DagSignalRecord> {
    Ok(DagSignalRecord {
        signal_id: row.signal_id,
        task_id: row.task_id,
        work_item_id: row.work_item_id,
        run_id: row.run_id,
        source_session_id: row.source_session_id,
        source: row.source,
        kind: row.kind,
        summary: row.summary,
        detail: row.detail,
        severity: row.severity,
        related_refs: serde_json::from_str(&row.related_refs)?,
        state: row.state,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub(crate) fn work_item_runtime_row_to_view(
    row: WorkItemRuntimeProjectionRow,
) -> WorkItemRuntimeView {
    WorkItemRuntimeView {
        current_run_id: row.current_run_id,
        current_state: row.current_state,
        current_attempt: row.current_attempt,
        ready_at: row.ready_at,
        blocked_reason: row.blocked_reason,
        outcome_state: row.outcome_state,
        outcome_reason: row.outcome_reason,
        replanned_from_state: row.replanned_from_state,
        retry_count: row.retry_count,
        max_retries: row.max_retries,
        priority: row.priority,
        optional: row.optional,
        parallelizable: row.parallelizable,
        session_id: row.session_id,
        turn_id: row.turn_id,
        updated_at: row.updated_at,
    }
}

pub(crate) fn dag_proposal_row_to_record(row: DagProposalRow) -> Result<DagProposal> {
    Ok(DagProposal {
        proposal_id: row.proposal_id,
        task_id: row.task_id,
        mode: row.mode,
        state: row.state,
        summary: row.summary,
        proposal_json: serde_json::from_str(&row.proposal_json)?,
        validation_json: serde_json::from_str(&row.validation_json)?,
        created_by_session_id: row.created_by_session_id,
        created_by_turn_id: row.created_by_turn_id,
        revision: row.revision,
        supersedes_proposal_id: row.supersedes_proposal_id,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
