use serde::Serialize;
use serde_json::Value;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::events::EventRow;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventView {
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub time: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStreamScope<'a> {
    Session {
        session_id: &'a str,
    },
    Turn {
        session_id: &'a str,
        turn_id: &'a str,
    },
}

impl TryFrom<EventRow> for EventView {
    type Error = pontia_core::Error;
    fn try_from(row: EventRow) -> Result<Self> {
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
}
