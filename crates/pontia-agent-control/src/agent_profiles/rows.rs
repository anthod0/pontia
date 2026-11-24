use super::AgentProfileView;
use pontia_storage_sqlite::models::agent_profiles::AgentProfileRow;

impl From<AgentProfileRow> for AgentProfileView {
    fn from(row: AgentProfileRow) -> Self {
        Self {
            profile_id: row.profile_id,
            name: row.name,
            description: row.description,
            client_type: row.client_type,
            client_key: row.client_key,
            model: row.model,
            reasoning_effort: row.reasoning_effort,
            use_yolo: row.use_yolo,
            system_prompt: row.system_prompt,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}
