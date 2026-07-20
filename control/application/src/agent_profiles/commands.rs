use super::validation::{ensure_not_builtin, is_unique_constraint, validate_request};
use super::*;
use pontia_storage_sqlite::repositories::agent_profiles::{
    ExecutionProfileWriteRecord, SqliteAgentProfileRepository,
};

impl AgentProfileService {
    pub async fn create_profile(
        &self,
        request: UpsertExecutionProfileRequest,
    ) -> Result<AgentProfileCommandOutcome> {
        if self.profile_exists(&request.profile_id).await? {
            return Err(Error::StateConflict(format!(
                "execution profile {} already exists; create a new version instead",
                request.profile_id
            )));
        }
        self.create_version(request).await
    }

    pub async fn create_profile_version(
        &self,
        profile_id: &str,
        request: UpsertExecutionProfileRequest,
    ) -> Result<AgentProfileCommandOutcome> {
        if profile_id != request.profile_id {
            return Err(Error::Domain(format!(
                "profile_id in path ({profile_id}) must match request profile_id ({})",
                request.profile_id
            )));
        }
        self.create_version(request).await
    }

    pub(super) async fn create_version(
        &self,
        request: UpsertExecutionProfileRequest,
    ) -> Result<AgentProfileCommandOutcome> {
        validate_request(&request)?;

        let result = SqliteAgentProfileRepository::new(self.pool.clone())
            .insert_version(execution_profile_write_record(&request)?)
            .await;

        if let Err(error) = result {
            if is_unique_constraint(&error) {
                return Err(Error::StateConflict(format!(
                    "execution profile {} version {} already exists",
                    request.profile_id, request.version
                )));
            }
            return Err(error);
        }

        let profile = self
            .get_version(&request.profile_id, &request.version)
            .await?
            .ok_or_else(|| Error::Domain("created execution profile missing".to_string()))?;
        let data = json!({ "agent_profile": profile });
        Ok(AgentProfileCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn update_version(
        &self,
        profile_id: &str,
        version: &str,
        request: UpsertExecutionProfileRequest,
    ) -> Result<AgentProfileCommandOutcome> {
        if profile_id != request.profile_id || version != request.version {
            return Err(Error::Domain(
                "profile_id and version in path must match request body".to_string(),
            ));
        }
        validate_request(&request)?;
        let current = self
            .get_version(profile_id, version)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!("agent profile {profile_id}@{version} not found"))
            })?;
        ensure_not_builtin(&current)?;

        SqliteAgentProfileRepository::new(self.pool.clone())
            .update_version(execution_profile_write_record(&request)?)
            .await?;

        let profile = self
            .get_version(profile_id, version)
            .await?
            .ok_or_else(|| Error::Domain("updated execution profile missing".to_string()))?;
        let data = json!({ "agent_profile": profile });
        Ok(AgentProfileCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn archive_version(
        &self,
        profile_id: &str,
        version: &str,
    ) -> Result<AgentProfileCommandOutcome> {
        let current = self
            .get_version(profile_id, version)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!("agent profile {profile_id}@{version} not found"))
            })?;
        ensure_not_builtin(&current)?;
        if current.active {
            SqliteAgentProfileRepository::new(self.pool.clone())
                .archive_version(profile_id, version)
                .await?;
        }
        let profile = self
            .get_version(profile_id, version)
            .await?
            .ok_or_else(|| Error::Domain("archived execution profile missing".to_string()))?;
        let data = json!({ "agent_profile": profile });
        Ok(AgentProfileCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn archive_profile(&self, profile_id: &str) -> Result<AgentProfileCommandOutcome> {
        let versions = self.list_versions(profile_id, true).await?;
        if versions.is_empty() {
            return Err(Error::NotFound(format!(
                "agent profile {profile_id} not found"
            )));
        }
        for version in &versions {
            ensure_not_builtin(version)?;
        }
        let archived_versions = SqliteAgentProfileRepository::new(self.pool.clone())
            .archive_active_versions(profile_id)
            .await?;
        let data = json!({ "profile_id": profile_id, "archived_versions": archived_versions });
        Ok(AgentProfileCommandOutcome {
            data,
            duplicate: false,
        })
    }
}

fn execution_profile_write_record(
    request: &UpsertExecutionProfileRequest,
) -> Result<ExecutionProfileWriteRecord> {
    Ok(ExecutionProfileWriteRecord {
        profile_id: request.profile_id.clone(),
        version: request.version.clone(),
        name: request.name.clone(),
        description: request.description.clone(),
        supported_client_types: serde_json::to_string(&request.supported_client_types)?,
        agent_kind: request.agent_kind.clone(),
        system_prompt_template: request.system_prompt_template.clone(),
        turn_prompt_template: request.turn_prompt_template.clone(),
        default_session_role: request.default_session_role.clone(),
        default_session_description: request.default_session_description.clone(),
        handle_prefix: request.handle_prefix.clone(),
        expected_output_schema: request.expected_output_schema.clone(),
        artifact_contract: serde_json::to_string(&request.artifact_contract)?,
        default_execution_policy: serde_json::to_string(&request.default_execution_policy)?,
        default_review_policy: serde_json::to_string(&request.default_review_policy)?,
        metadata: serde_json::to_string(&request.metadata)?,
    })
}
