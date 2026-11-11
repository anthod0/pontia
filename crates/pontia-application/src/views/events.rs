use serde::Serialize;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::events::{EventRow, EventStreamRow};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventView {
    #[serde(flatten)]
    pub fact: pontia_agent_control::EventView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<crate::session_files::SharedFile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventStreamItem {
    pub rowid: i64,
    pub event: EventView,
}

fn stream_row_to_view(row: EventStreamRow) -> Result<EventView> {
    let fact = pontia_agent_control::EventView::try_from(EventRow {
        event_id: row.event_id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        source: row.source,
        event_type: row.event_type,
        occurred_at: row.occurred_at,
        payload: row.payload,
    })?;
    Ok(EventView { fact, file: None })
}

pub(crate) fn row_to_item(row: EventStreamRow) -> Result<EventStreamItem> {
    let rowid = row.rowid;
    let event = stream_row_to_view(row)?;
    Ok(EventStreamItem { rowid, event })
}
