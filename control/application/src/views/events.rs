use serde::Serialize;
use serde_json::Value;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::events::{EventRow, EventStreamRow, TaskEventStreamRow};

use super::tasks::{TaskEventView, stream_event_row_to_view};

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

#[derive(Debug, Clone, PartialEq)]
pub struct EventStreamItem {
    pub rowid: i64,
    pub event: EventView,
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

#[derive(Debug, Clone, PartialEq)]
pub struct TaskEventStreamItem {
    pub rowid: i64,
    pub event: TaskEventView,
}

pub(crate) fn row_to_view(row: EventRow) -> Result<EventView> {
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

fn stream_row_to_view(row: EventStreamRow) -> Result<EventView> {
    row_to_view(EventRow {
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

pub(crate) fn row_to_task_item(row: TaskEventStreamRow) -> Result<TaskEventStreamItem> {
    let rowid = row.rowid;
    let event = stream_event_row_to_view(row)?;
    Ok(TaskEventStreamItem { rowid, event })
}
