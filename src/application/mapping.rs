use super::*;
use crate::storage::sqlite::models::{
    artifacts::ArtifactRow,
    dag::{DagProposalRow, DagSignalRow, WorkItemRunRow, WorkItemRuntimeProjectionRow},
    events::{EventRow, EventStreamRow, TaskEventStreamRow},
    sessions::SessionRow,
    tasks::{TaskEventRow, TaskRow},
    turns::TurnRow,
    workspaces::WorkspaceRow,
};

pub(crate) fn session_row_to_view(row: SessionRow) -> Result<SessionView> {
    let metadata: Value = serde_json::from_str(&row.metadata)?;
    let context_usage = metadata
        .get("context_usage")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?;
    let model = metadata
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok(SessionView {
        session_id: row.session_id,
        client_type: row.client_type,
        title: row.title,
        handle: row.handle,
        role: row.role,
        description: row.description,
        execution_profile_id: row.execution_profile_id,
        execution_profile_version: row.execution_profile_version,
        state: row.state,
        current_turn_id: row.current_turn_id,
        workspace_id: row.workspace_id,
        workspace: row.workspace_ref,
        capabilities: SessionCapabilities::default(),
        model,
        context_usage,
        created_at: row.created_at,
        updated_at: row.updated_at,
        metadata,
    })
}

pub(crate) fn workspace_row_to_view(row: WorkspaceRow) -> Result<WorkspaceView> {
    Ok(WorkspaceView {
        workspace_id: row.workspace_id,
        canonical_path: row.canonical_path,
        display_path: row.display_path,
        name: row.name,
        state: row.state,
        metadata: serde_json::from_str(&row.metadata)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
        last_used_at: row.last_used_at,
    })
}

pub(crate) fn row_to_workspace_git_status_view(
    row: sqlx::sqlite::SqliteRow,
) -> Result<WorkspaceGitStatusView> {
    Ok(WorkspaceGitStatusView {
        workspace_id: row.try_get("workspace_id")?,
        repo_root: row.try_get("repo_root")?,
        branch: row.try_get("branch")?,
        upstream: row.try_get("upstream")?,
        ahead: row.try_get("ahead")?,
        behind: row.try_get("behind")?,
        staged_count: row.try_get("staged_count")?,
        unstaged_count: row.try_get("unstaged_count")?,
        untracked_count: row.try_get("untracked_count")?,
        conflicted_count: row.try_get("conflicted_count")?,
        clean: row.try_get("clean")?,
        state: row.try_get("state")?,
        failure: row.try_get("failure")?,
        observed_at: row.try_get("observed_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub(crate) fn task_row_to_view(row: TaskRow) -> Result<TaskView> {
    Ok(TaskView {
        task_id: row.task_id,
        state: row.state,
        input: row.input,
        workspace_id: row.workspace_id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        routing_state: row.routing_state,
        routing_reason: row.routing_reason,
        routing_confidence: row.routing_confidence,
        metadata: serde_json::from_str(&row.metadata)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub(crate) fn task_event_row_to_view(row: TaskEventRow) -> Result<TaskEventView> {
    Ok(TaskEventView {
        event_id: row.event_id,
        task_id: row.task_id,
        event_type: row.event_type,
        payload: serde_json::from_str(&row.payload)?,
        created_at: row.created_at,
    })
}

pub(crate) fn task_event_stream_row_to_view(row: TaskEventStreamRow) -> Result<TaskEventView> {
    Ok(TaskEventView {
        event_id: row.event_id,
        task_id: row.task_id,
        event_type: row.event_type,
        payload: serde_json::from_str(&row.payload)?,
        created_at: row.created_at,
    })
}

pub(crate) fn dag_proposal_row_to_view(row: DagProposalRow) -> Result<DagProposalView> {
    Ok(DagProposalView {
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

pub(crate) fn turn_row_to_view(row: TurnRow) -> Result<TurnView> {
    let metadata_json: Value = serde_json::from_str(&row.metadata)?;
    let artifact_ids = metadata_json
        .get("artifact_ids")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    Ok(TurnView {
        turn_id: row.turn_id,
        session_id: row.session_id,
        state: row.state,
        input: TurnInputView {
            summary: row.input_summary,
            artifact_id: metadata_json
                .get("input_artifact_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        },
        output: TurnOutputView {
            summary: row.output_summary,
            artifact_ids,
        },
        failure: row.failure_message,
        created_at: row.created_at,
        started_at: None,
        completed_at: None,
        metadata: metadata_json,
    })
}

pub(crate) fn row_to_inbox_message_view(row: sqlx::sqlite::SqliteRow) -> Result<InboxMessageView> {
    let metadata: String = row.try_get("metadata")?;

    Ok(InboxMessageView {
        message_id: row.try_get("message_id")?,
        session_id: row.try_get("session_id")?,
        state: row.try_get("state")?,
        delivery_policy: row.try_get("delivery_policy")?,
        input: InboxInputView {
            summary: row.try_get("input_summary")?,
        },
        metadata: serde_json::from_str(&metadata)?,
        turn_id: row.try_get("turn_id")?,
        superseded_by_message_id: row.try_get("superseded_by_message_id")?,
        failure_message: row.try_get("failure_message")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        dispatched_at: row.try_get("dispatched_at")?,
        cancelled_at: row.try_get("cancelled_at")?,
    })
}

pub(crate) fn event_row_to_view(row: EventRow) -> Result<EventView> {
    Ok(EventView {
        event_id: row.event_id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        source: row.source,
        event_type: row.event_type,
        time: row.occurred_at,
        payload: serde_json::from_str(&row.payload)?,
    })
}

pub(crate) fn event_stream_row_to_view(row: EventStreamRow) -> Result<EventView> {
    event_row_to_view(EventRow {
        event_id: row.event_id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        source: row.source,
        event_type: row.event_type,
        occurred_at: row.occurred_at,
        payload: row.payload,
    })
}

pub(crate) fn event_stream_row_to_item(row: EventStreamRow) -> Result<EventStreamItem> {
    let rowid = row.rowid;
    let event = event_stream_row_to_view(row)?;
    Ok(EventStreamItem { rowid, event })
}

pub(crate) fn artifact_row_to_view(row: ArtifactRow) -> Result<ArtifactView> {
    let mut metadata_json: Value = serde_json::from_str(&row.metadata)?;
    remove_internal_metadata_fields(&mut metadata_json);

    Ok(ArtifactView {
        artifact_id: row.artifact_id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        kind: row.kind,
        name: row.name,
        size_bytes: row.size_bytes,
        preview: metadata_json
            .get("preview")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        created_at: row.created_at,
        metadata: metadata_json,
    })
}

pub(crate) fn row_to_session(row: sqlx::sqlite::SqliteRow) -> Result<SessionProjection> {
    let metadata: String = row.try_get("metadata")?;
    let state: String = row.try_get("state")?;

    Ok(SessionProjection {
        session_id: row.try_get("session_id")?,
        client_type: row.try_get("client_type")?,
        title: row.try_get("title")?,
        handle: row.try_get("handle")?,
        role: row.try_get("role")?,
        description: row.try_get("description")?,
        execution_profile_id: row.try_get("execution_profile_id")?,
        execution_profile_version: row.try_get("execution_profile_version")?,
        state: SessionState::from_str(&state)?,
        current_turn_id: row.try_get("current_turn_id")?,
        state_version: row.try_get("state_version")?,
        metadata: serde_json::from_str(&metadata)?,
    })
}

pub(crate) fn row_to_turn(row: sqlx::sqlite::SqliteRow) -> Result<TurnProjection> {
    let metadata: String = row.try_get("metadata")?;
    let state: String = row.try_get("state")?;

    Ok(TurnProjection {
        turn_id: row.try_get("turn_id")?,
        session_id: row.try_get("session_id")?,
        state: TurnState::from_str(&state)?,
        state_version: row.try_get("state_version")?,
        metadata: serde_json::from_str(&metadata)?,
    })
}

pub(crate) fn row_to_event(row: sqlx::sqlite::SqliteRow) -> Result<DomainEvent> {
    let payload: String = row.try_get("payload")?;
    let source: String = row.try_get("source")?;
    let event_type: String = row.try_get("event_type")?;
    let occurred_at: String = row.try_get("occurred_at")?;

    Ok(DomainEvent {
        event_id: row.try_get("event_id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        source: EventSource::from_str(&source)?,
        client_type: row.try_get("client_type")?,
        event_type: EventType::from_str(&event_type)?,
        occurred_at: time::OffsetDateTime::parse(
            &occurred_at,
            &time::format_description::well_known::Rfc3339,
        )
        .map_err(|err| crate::error::Error::Domain(format!("invalid event timestamp: {err}")))?,
        seq: row.try_get("seq")?,
        payload: serde_json::from_str(&payload)?,
    })
}
