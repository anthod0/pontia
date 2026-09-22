use serde::Serialize;
use serde_json::Value;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::turns::TurnRow;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnInputView {
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnOutputView {
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnView {
    pub turn_id: String,
    pub session_id: String,
    pub parent_turn_id: Option<String>,
    pub topology_status: String,
    pub state: String,
    pub input: TurnInputView,
    pub output: TurnOutputView,
    pub failure: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub metadata: Value,
}

pub(crate) fn row_to_view(row: TurnRow) -> Result<TurnView> {
    let metadata = serde_json::from_str(&row.metadata)?;

    Ok(TurnView {
        turn_id: row.turn_id,
        session_id: row.session_id,
        parent_turn_id: row.parent_turn_id,
        topology_status: row.topology_status,
        state: row.state,
        input: TurnInputView {
            summary: row.input_summary,
        },
        output: TurnOutputView {
            summary: row.output_summary,
        },
        failure: None,
        created_at: row.created_at,
        started_at: None,
        completed_at: None,
        metadata,
    })
}
