use pontia_agent_control::EventView;
use pontia_core::error::Result;
use pontia_storage_sqlite::models::events::{EventRow, EventStreamRow};

#[derive(Debug, Clone, PartialEq)]
pub struct EventStreamItem {
    pub rowid: i64,
    pub event: EventView,
}

fn stream_row_to_view(row: EventStreamRow) -> Result<EventView> {
    EventView::try_from(EventRow {
        event_id: row.event_id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        source: row.source,
        event_type: row.event_type,
        occurred_at: row.occurred_at,
        payload: row.payload,
    })
}

pub(crate) fn row_to_item(row: EventStreamRow) -> Result<EventStreamItem> {
    let rowid = row.rowid;
    let event = stream_row_to_view(row)?;
    Ok(EventStreamItem { rowid, event })
}
