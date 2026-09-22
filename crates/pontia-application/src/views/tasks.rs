use serde::Serialize;
use serde_json::Value;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::{
    events::TaskEventStreamRow,
    tasks::{TaskEventRow, TaskRow},
};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskView {
    pub task_id: String,
    pub state: String,
    pub input: String,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub routing_state: String,
    pub routing_reason: Option<String>,
    pub routing_confidence: Option<f64>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskEventView {
    pub event_id: String,
    pub task_id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}

pub(crate) fn row_to_view(row: TaskRow) -> Result<TaskView> {
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

pub(crate) fn event_row_to_view(row: TaskEventRow) -> Result<TaskEventView> {
    task_event_to_view(
        row.event_id,
        row.task_id,
        row.event_type,
        row.payload,
        row.created_at,
    )
}

pub(crate) fn stream_event_row_to_view(row: TaskEventStreamRow) -> Result<TaskEventView> {
    task_event_to_view(
        row.event_id,
        row.task_id,
        row.event_type,
        row.payload,
        row.created_at,
    )
}

fn task_event_to_view(
    event_id: String,
    task_id: String,
    event_type: String,
    payload: String,
    created_at: String,
) -> Result<TaskEventView> {
    Ok(TaskEventView {
        event_id,
        task_id,
        event_type,
        payload: serde_json::from_str(&payload)?,
        created_at,
    })
}
