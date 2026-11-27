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
    pub steer_target_turn_id: Option<String>,
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
        state: "pending".into(),
        delivery_policy: "after_idle".into(),
        input: InboxInputView {
            summary: row.input_summary,
        },
        metadata: serde_json::from_str(&row.metadata)?,
        branch_target_turn_id: None,
        steer_target_turn_id: None,
        turn_id: None,
        superseded_by_message_id: None,
        failure_message: None,
        updated_at: row.created_at.clone(),
        created_at: row.created_at,
        dispatched_at: None,
        cancelled_at: None,
    })
}

pub(crate) fn dispatched_view(
    mut view: InboxMessageView,
    turn_id: Option<String>,
) -> InboxMessageView {
    view.state = "dispatched".into();
    view.dispatched_at = Some(view.created_at.clone());
    view.turn_id = turn_id;
    view
}

pub(crate) fn cancelled_view(mut view: InboxMessageView) -> InboxMessageView {
    view.state = "cancelled".into();
    view
}
