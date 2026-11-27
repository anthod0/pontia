use serde::Serialize;
use serde_json::Value;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::inbox::InboxMessageRow;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueuedMessageView {
    pub message_id: String,
    pub session_id: String,
    pub input: String,
    pub metadata: Value,
    pub created_at: String,
}

pub(crate) fn row_to_view(row: InboxMessageRow) -> Result<QueuedMessageView> {
    Ok(QueuedMessageView {
        message_id: row.message_id,
        session_id: row.session_id,
        input: row.input_summary,
        metadata: serde_json::from_str(&row.metadata)?,
        created_at: row.created_at,
    })
}
