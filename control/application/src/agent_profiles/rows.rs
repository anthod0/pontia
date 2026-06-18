use super::*;
use pontia_storage_sqlite::models::agent_profiles::ExecutionProfileRow;

pub(super) fn row_to_execution_profile_view(
    row: ExecutionProfileRow,
) -> Result<ExecutionProfileView> {
    Ok(ExecutionProfileView {
        profile_id: row.profile_id,
        version: row.version,
        name: row.name,
        description: row.description,
        supported_client_types: serde_json::from_str(&row.supported_client_types)?,
        agent_kind: row.agent_kind,
        system_prompt_template: row.system_prompt_template,
        turn_prompt_template: row.turn_prompt_template,
        default_session_role: row.default_session_role,
        default_session_description: row.default_session_description,
        handle_prefix: row.handle_prefix,
        expected_output_schema: row.expected_output_schema,
        artifact_contract: serde_json::from_str(&row.artifact_contract)?,
        default_execution_policy: serde_json::from_str(&row.default_execution_policy)?,
        default_review_policy: serde_json::from_str(&row.default_review_policy)?,
        metadata: serde_json::from_str(&row.metadata)?,
        active: row.active,
        archived_at: row.archived_at,
        archived_reason: row.archived_reason,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
