use super::*;
use pontia_storage_sqlite::models::{
    events::{DomainEventRow, EventRow, EventStreamRow, TaskEventStreamRow},
    git_status::WorkspaceGitStatusRow,
    inbox::InboxMessageRow,
    sessions::{SessionProjectionRow, SessionRow},
    tasks::{TaskEventRow, TaskRow},
    turns::{TurnProjectionRow, TurnRow},
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
        pinned_at: row.pinned_at,
        archived_at: row.archived_at,
        capabilities: SessionCapabilities::default(),
        model,
        context_usage,
        lineage: None,
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
    row: WorkspaceGitStatusRow,
) -> Result<WorkspaceGitStatusView> {
    Ok(WorkspaceGitStatusView {
        workspace_id: row.workspace_id,
        repo_root: row.repo_root,
        branch: row.branch,
        upstream: row.upstream,
        ahead: row.ahead,
        behind: row.behind,
        staged_count: row.staged_count,
        unstaged_count: row.unstaged_count,
        untracked_count: row.untracked_count,
        conflicted_count: row.conflicted_count,
        clean: row.clean,
        state: row.state,
        failure: row.failure,
        observed_at: row.observed_at,
        updated_at: row.updated_at,
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

pub(crate) fn task_event_stream_row_to_item(
    row: TaskEventStreamRow,
) -> Result<TaskEventStreamItem> {
    let rowid = row.rowid;
    let event = task_event_stream_row_to_view(row)?;
    Ok(TaskEventStreamItem { rowid, event })
}

pub(crate) fn turn_row_to_view(row: TurnRow) -> Result<TurnView> {
    let metadata_json: Value = serde_json::from_str(&row.metadata)?;

    Ok(TurnView {
        turn_id: row.turn_id,
        session_id: row.session_id,
        state: row.state,
        input: TurnInputView {
            summary: row.input_summary,
        },
        output: TurnOutputView {
            summary: row.output_summary,
        },
        failure: row.failure_message,
        created_at: row.created_at,
        started_at: None,
        completed_at: None,
        metadata: metadata_json,
    })
}

pub(crate) fn row_to_inbox_message_view(row: InboxMessageRow) -> Result<InboxMessageView> {
    Ok(InboxMessageView {
        message_id: row.message_id,
        session_id: row.session_id,
        state: row.state,
        delivery_policy: row.delivery_policy,
        input: InboxInputView {
            summary: row.input_summary,
        },
        metadata: serde_json::from_str(&row.metadata)?,
        turn_id: row.turn_id,
        superseded_by_message_id: row.superseded_by_message_id,
        failure_message: row.failure_message,
        created_at: row.created_at,
        updated_at: row.updated_at,
        dispatched_at: row.dispatched_at,
        cancelled_at: row.cancelled_at,
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

pub(crate) fn row_to_session(row: SessionProjectionRow) -> Result<SessionProjection> {
    Ok(SessionProjection {
        session_id: row.session_id,
        client_type: row.client_type,
        title: row.title,
        handle: row.handle,
        role: row.role,
        description: row.description,
        execution_profile_id: row.execution_profile_id,
        execution_profile_version: row.execution_profile_version,
        state: SessionState::from_str(&row.state)?,
        current_turn_id: row.current_turn_id,
        state_version: row.state_version,
        metadata: serde_json::from_str(&row.metadata)?,
    })
}

pub(crate) fn row_to_turn(row: TurnProjectionRow) -> Result<TurnProjection> {
    Ok(TurnProjection {
        turn_id: row.turn_id,
        session_id: row.session_id,
        state: TurnState::from_str(&row.state)?,
        state_version: row.state_version,
        metadata: serde_json::from_str(&row.metadata)?,
    })
}

pub(crate) fn row_to_event(row: DomainEventRow) -> Result<DomainEvent> {
    Ok(DomainEvent {
        event_id: row.event_id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        source: EventSource::from_str(&row.source)?,
        client_type: row.client_type,
        event_type: EventType::from_str(&row.event_type)?,
        occurred_at: time::OffsetDateTime::parse(
            &row.occurred_at,
            &time::format_description::well_known::Rfc3339,
        )
        .map_err(|err| {
            pontia_core::error::Error::Domain(format!("invalid event timestamp: {err}"))
        })?,
        seq: row.seq,
        payload: serde_json::from_str(&row.payload)?,
    })
}
