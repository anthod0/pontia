use serde::Serialize;
use serde_json::Value;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::inbox::InboxMessageRow;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InboxInputView {
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InboxMessageView {
    pub message_id: String,
    pub session_id: String,
    pub state: String,
    pub delivery_policy: String,
    pub input: InboxInputView,
    pub metadata: Value,
    pub branch_target_turn_id: Option<String>,
    pub turn_id: Option<String>,
    pub superseded_by_message_id: Option<String>,
    pub failure_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub dispatched_at: Option<String>,
    pub cancelled_at: Option<String>,
}

pub(crate) fn row_to_view(row: InboxMessageRow) -> Result<InboxMessageView> {
    Ok(InboxMessageView {
        message_id: row.message_id,
        session_id: row.session_id,
        state: row.state,
        delivery_policy: row.delivery_policy,
        input: InboxInputView {
            summary: row.input_summary,
        },
        metadata: serde_json::from_str(&row.metadata)?,
        branch_target_turn_id: row.branch_target_turn_id,
        turn_id: row.turn_id,
        superseded_by_message_id: row.superseded_by_message_id,
        failure_message: row.failure_message,
        created_at: row.created_at,
        updated_at: row.updated_at,
        dispatched_at: row.dispatched_at,
        cancelled_at: row.cancelled_at,
    })
}
