use pontia_core::{
    domain::{DomainEvent, ProjectionState},
    error::{Error, Result},
};
use pontia_storage_sqlite::repositories::{
    events::{EventInsertRecord, SqliteEventRepository},
    sessions::{SessionProjectionUpsertRecord, SqliteSessionRepository},
    turns::{SqliteTurnRepository, TurnProjectionUpsertRecord},
};
use sqlx::{Sqlite, Transaction};

pub(super) async fn insert_event_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    event: &DomainEvent,
) -> Result<()> {
    let payload = serde_json::to_string(&event.payload)?;
    let timeline_boundary = event
        .timeline_boundary
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let turn_topology = event
        .topology
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let occurred_at = event
        .occurred_at
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|err| Error::Domain(format!("invalid event timestamp: {err}")))?;

    SqliteEventRepository::insert_event_in_tx(
        tx,
        EventInsertRecord {
            event_id: event.event_id.clone(),
            session_id: event.session_id.clone(),
            turn_id: event.turn_id.clone(),
            source: event.source.to_string(),
            client_type: event.client_type.clone(),
            event_type: event.event_type.to_string(),
            occurred_at,
            payload,
            timeline_boundary,
            turn_topology,
        },
    )
    .await
}

pub(super) async fn persist_projections_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    projection: &ProjectionState,
    state_version: i64,
) -> Result<()> {
    for session in projection.sessions() {
        let metadata = serde_json::to_string(&session.metadata)?;
        SqliteSessionRepository::upsert_projection_in_tx(
            tx,
            SessionProjectionUpsertRecord {
                session_id: session.session_id.clone(),
                client_type: session.client_type.clone(),
                title: session.title.clone(),
                handle: session.handle.clone(),
                role: session.role.clone(),
                description: session.description.clone(),
                execution_profile_id: session.execution_profile_id.clone(),
                execution_profile_version: session.execution_profile_version.clone(),
                state: session.state.to_string(),
                current_turn_id: session.current_turn_id.clone(),
                state_version,
                metadata,
            },
        )
        .await?;
    }

    for turn in projection.turns() {
        let metadata = serde_json::to_string(&turn.metadata)?;
        SqliteTurnRepository::upsert_projection_in_tx(
            tx,
            TurnProjectionUpsertRecord {
                turn_id: turn.turn_id.clone(),
                session_id: turn.session_id.clone(),
                head_cursor: turn.head_cursor.clone(),
                tail_cursor: turn.tail_cursor.clone(),
                parent_turn_id: turn.topology.parent_turn_id().map(ToString::to_string),
                topology_status: turn.topology.status().to_string(),
                state: turn.state.to_string(),
                state_version: turn.state_version,
                input_summary: turn.input_summary.clone(),
                output_summary: turn.output_summary.clone(),
                metadata,
            },
        )
        .await?;
    }

    Ok(())
}
