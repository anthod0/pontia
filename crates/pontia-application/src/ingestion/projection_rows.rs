use std::str::FromStr;

use pontia_core::{
    domain::{
        DomainEvent, EventSource, EventType, SessionProjection, SessionState, TurnProjection,
        TurnState, TurnTopology,
    },
    error::{Error, Result},
};
use pontia_storage_sqlite::models::{
    events::DomainEventRow, sessions::SessionProjectionRow, turns::TurnProjectionRow,
};

pub(super) fn session_from_row(row: SessionProjectionRow) -> Result<SessionProjection> {
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

pub(super) fn turn_from_row(row: TurnProjectionRow) -> Result<TurnProjection> {
    let topology = TurnTopology::from_parts(&row.topology_status, row.parent_turn_id)?;
    Ok(TurnProjection {
        turn_id: row.turn_id,
        session_id: row.session_id,
        head_cursor: row.head_cursor,
        tail_cursor: row.tail_cursor,
        topology,
        state: TurnState::from_str(&row.state)?,
        state_version: row.state_version,
        input_summary: row.input_summary,
        output_summary: row.output_summary,
        metadata: serde_json::from_str(&row.metadata)?,
    })
}

pub(super) fn event_from_row(row: DomainEventRow) -> Result<DomainEvent> {
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
        .map_err(|err| Error::Domain(format!("invalid event timestamp: {err}")))?,
        payload: serde_json::from_str(&row.payload)?,
        timeline_boundary: row
            .timeline_boundary
            .map(|boundary| serde_json::from_str(&boundary))
            .transpose()?,
        topology: row
            .turn_topology
            .map(|topology| serde_json::from_str(&topology))
            .transpose()?,
    })
}
