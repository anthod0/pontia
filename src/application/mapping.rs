use super::*;

pub(crate) fn row_to_session_view(row: sqlx::sqlite::SqliteRow) -> Result<SessionView> {
    let metadata: String = row.try_get("metadata")?;

    Ok(SessionView {
        session_id: row.try_get("session_id")?,
        client_type: row.try_get("client_type")?,
        state: row.try_get("state")?,
        current_turn_id: row.try_get("current_turn_id")?,
        workspace_id: row.try_get("workspace_id")?,
        workspace: row.try_get("workspace_ref")?,
        capabilities: SessionCapabilities::default(),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        metadata: serde_json::from_str(&metadata)?,
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

pub(crate) fn row_to_task_view(row: sqlx::sqlite::SqliteRow) -> Result<TaskView> {
    let metadata: String = row.try_get("metadata")?;

    Ok(TaskView {
        task_id: row.try_get("task_id")?,
        state: row.try_get("state")?,
        input: row.try_get("input")?,
        workspace_id: row.try_get("workspace_id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        routing_state: row.try_get("routing_state")?,
        routing_reason: row.try_get("routing_reason")?,
        routing_confidence: row.try_get("routing_confidence")?,
        metadata: serde_json::from_str(&metadata)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
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
