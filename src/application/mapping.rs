use super::*;
use crate::storage::sqlite::models::tasks::{TaskEventRow, TaskRow};

pub(crate) fn row_to_session_view(row: sqlx::sqlite::SqliteRow) -> Result<SessionView> {
    let metadata: String = row.try_get("metadata")?;
    let metadata: Value = serde_json::from_str(&metadata)?;
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
        session_id: row.try_get("session_id")?,
        client_type: row.try_get("client_type")?,
        title: row.try_get("title")?,
        handle: row.try_get("handle")?,
        role: row.try_get("role")?,
        description: row.try_get("description")?,
        execution_profile_id: row.try_get("execution_profile_id")?,
        execution_profile_version: row.try_get("execution_profile_version")?,
        state: row.try_get("state")?,
        current_turn_id: row.try_get("current_turn_id")?,
        workspace_id: row.try_get("workspace_id")?,
        workspace: row.try_get("workspace_ref")?,
        capabilities: SessionCapabilities::default(),
        model,
        context_usage,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        metadata,
    })
}

pub(crate) fn row_to_workspace_view(row: sqlx::sqlite::SqliteRow) -> Result<WorkspaceView> {
    let metadata: String = row.try_get("metadata")?;

    Ok(WorkspaceView {
        workspace_id: row.try_get("workspace_id")?,
        canonical_path: row.try_get("canonical_path")?,
        display_path: row.try_get("display_path")?,
        name: row.try_get("name")?,
        state: row.try_get("state")?,
        metadata: serde_json::from_str(&metadata)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        last_used_at: row.try_get("last_used_at")?,
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

pub(crate) fn row_to_task_event_view(row: sqlx::sqlite::SqliteRow) -> Result<TaskEventView> {
    let payload: String = row.try_get("payload")?;

    Ok(TaskEventView {
        event_id: row.try_get("event_id")?,
        task_id: row.try_get("task_id")?,
        event_type: row.try_get("event_type")?,
        payload: serde_json::from_str(&payload)?,
        created_at: row.try_get("created_at")?,
    })
}

pub(crate) fn row_to_dag_proposal_view(row: sqlx::sqlite::SqliteRow) -> Result<DagProposalView> {
    let proposal_json: String = row.try_get("proposal_json")?;
    let validation_json: String = row.try_get("validation_json")?;

    Ok(DagProposalView {
        proposal_id: row.try_get("proposal_id")?,
        task_id: row.try_get("task_id")?,
        mode: row.try_get("mode")?,
        state: row.try_get("state")?,
        summary: row.try_get("summary")?,
        proposal_json: serde_json::from_str(&proposal_json)?,
        validation_json: serde_json::from_str(&validation_json)?,
        created_by_session_id: row.try_get("created_by_session_id")?,
        created_by_turn_id: row.try_get("created_by_turn_id")?,
        revision: row.try_get("revision")?,
        supersedes_proposal_id: row.try_get("supersedes_proposal_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
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

pub(crate) fn row_to_work_item_run_record(
    row: sqlx::sqlite::SqliteRow,
) -> Result<WorkItemRunRecord> {
    let failure: Option<String> = row.try_get("failure")?;
    Ok(WorkItemRunRecord {
        run_id: row.try_get("run_id")?,
        work_item_id: row.try_get("work_item_id")?,
        task_id: row.try_get("task_id")?,
        attempt: row.try_get("attempt")?,
        state: row.try_get("state")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        client_type: row.try_get("client_type")?,
        execution_profile_id: row.try_get("execution_profile_id")?,
        execution_profile_version: row.try_get("execution_profile_version")?,
        rendered_prompt_ref: row.try_get("rendered_prompt_ref")?,
        output_summary: row.try_get("output_summary")?,
        failure: failure
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
    })
}

pub(crate) fn row_to_dag_signal_record(row: sqlx::sqlite::SqliteRow) -> Result<DagSignalRecord> {
    let related_refs: String = row.try_get("related_refs")?;
    Ok(DagSignalRecord {
        signal_id: row.try_get("signal_id")?,
        task_id: row.try_get("task_id")?,
        work_item_id: row.try_get("work_item_id")?,
        run_id: row.try_get("run_id")?,
        source_session_id: row.try_get("source_session_id")?,
        source: row.try_get("source")?,
        kind: row.try_get("kind")?,
        summary: row.try_get("summary")?,
        detail: row.try_get("detail")?,
        severity: row.try_get("severity")?,
        related_refs: serde_json::from_str(&related_refs)?,
        state: row.try_get("state")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub(crate) fn row_to_dag_proposal(row: sqlx::sqlite::SqliteRow) -> Result<DagProposal> {
    let proposal_json: String = row.try_get("proposal_json")?;
    let validation_json: String = row.try_get("validation_json")?;
    Ok(DagProposal {
        proposal_id: row.try_get("proposal_id")?,
        task_id: row.try_get("task_id")?,
        mode: row.try_get("mode")?,
        state: row.try_get("state")?,
        summary: row.try_get("summary")?,
        proposal_json: serde_json::from_str(&proposal_json)?,
        validation_json: serde_json::from_str(&validation_json)?,
        created_by_session_id: row.try_get("created_by_session_id")?,
        created_by_turn_id: row.try_get("created_by_turn_id")?,
        revision: row.try_get("revision")?,
        supersedes_proposal_id: row.try_get("supersedes_proposal_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub(crate) fn row_to_turn_view(row: sqlx::sqlite::SqliteRow) -> Result<TurnView> {
    let metadata: String = row.try_get("metadata")?;
    let metadata_json: Value = serde_json::from_str(&metadata)?;
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
        turn_id: row.try_get("turn_id")?,
        session_id: row.try_get("session_id")?,
        state: row.try_get("state")?,
        input: TurnInputView {
            summary: row.try_get("input_summary")?,
            artifact_id: metadata_json
                .get("input_artifact_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        },
        output: TurnOutputView {
            summary: row.try_get("output_summary")?,
            artifact_ids,
        },
        failure: row.try_get("failure_message")?,
        created_at: row.try_get("created_at")?,
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

pub(crate) fn row_to_event_view(row: sqlx::sqlite::SqliteRow) -> Result<EventView> {
    let payload: String = row.try_get("payload")?;

    Ok(EventView {
        event_id: row.try_get("event_id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        source: row.try_get("source")?,
        event_type: row.try_get("event_type")?,
        time: row.try_get("occurred_at")?,
        payload: serde_json::from_str(&payload)?,
    })
}

pub(crate) fn row_to_event_stream_item(row: sqlx::sqlite::SqliteRow) -> Result<EventStreamItem> {
    let rowid = row.try_get("rowid")?;
    let event = row_to_event_view(row)?;
    Ok(EventStreamItem { rowid, event })
}

pub(crate) fn row_to_artifact_view(row: sqlx::sqlite::SqliteRow) -> Result<ArtifactView> {
    let metadata: String = row.try_get("metadata")?;
    let mut metadata_json: Value = serde_json::from_str(&metadata)?;
    remove_internal_metadata_fields(&mut metadata_json);

    Ok(ArtifactView {
        artifact_id: row.try_get("artifact_id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        kind: row.try_get("kind")?,
        name: row.try_get("name")?,
        size_bytes: row.try_get("size_bytes")?,
        preview: metadata_json
            .get("preview")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        created_at: row.try_get("created_at")?,
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
