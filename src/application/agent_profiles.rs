use super::*;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExecutionProfileView {
    pub profile_id: String,
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    pub supported_client_types: Vec<String>,
    pub system_prompt_template: Option<String>,
    pub turn_prompt_template: Option<String>,
    pub default_session_role: Option<String>,
    pub default_session_description: Option<String>,
    pub handle_prefix: Option<String>,
    pub expected_output_schema: Option<String>,
    pub artifact_contract: Value,
    pub default_execution_policy: Value,
    pub default_review_policy: Value,
    pub metadata: Value,
    pub active: bool,
    pub archived_at: Option<String>,
    pub archived_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UpsertExecutionProfileRequest {
    pub profile_id: String,
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub supported_client_types: Vec<String>,
    pub system_prompt_template: Option<String>,
    pub turn_prompt_template: Option<String>,
    pub default_session_role: Option<String>,
    pub default_session_description: Option<String>,
    pub handle_prefix: Option<String>,
    pub expected_output_schema: Option<String>,
    #[serde(default = "empty_object")]
    pub artifact_contract: Value,
    #[serde(default = "empty_object")]
    pub default_execution_policy: Value,
    #[serde(default = "empty_object")]
    pub default_review_policy: Value,
    #[serde(default = "empty_object")]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentProfileCommandOutcome {
    pub data: Value,
    pub duplicate: bool,
}

fn empty_object() -> Value {
    json!({})
}

#[derive(Clone)]
pub struct AgentProfileService {
    pool: SqlitePool,
}

impl AgentProfileService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_latest(&self) -> Result<Vec<ExecutionProfileView>> {
        let rows = sqlx::query(
            r#"SELECT profile_id, version, name, description, supported_client_types,
                      system_prompt_template, turn_prompt_template, default_session_role,
                      default_session_description, handle_prefix,
                      expected_output_schema, artifact_contract, default_execution_policy,
                      default_review_policy, metadata, active, archived_at, archived_reason,
                      created_at, updated_at
               FROM execution_profiles ep
               WHERE active = 1 AND archived_at IS NULL AND rowid = (
                   SELECT max(rowid) FROM execution_profiles latest
                   WHERE latest.profile_id = ep.profile_id
                     AND latest.active = 1
                     AND latest.archived_at IS NULL
               )
               ORDER BY profile_id"#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(row_to_execution_profile_view)
            .collect()
    }

    pub async fn list_latest_including_archived(&self) -> Result<Vec<ExecutionProfileView>> {
        let rows = sqlx::query(
            r#"SELECT profile_id, version, name, description, supported_client_types,
                      system_prompt_template, turn_prompt_template, default_session_role,
                      default_session_description, handle_prefix,
                      expected_output_schema, artifact_contract, default_execution_policy,
                      default_review_policy, metadata, active, archived_at, archived_reason,
                      created_at, updated_at
               FROM execution_profiles ep
               WHERE rowid = (
                   SELECT max(rowid) FROM execution_profiles latest
                   WHERE latest.profile_id = ep.profile_id
               )
               ORDER BY profile_id"#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(row_to_execution_profile_view)
            .collect()
    }

    pub async fn get_latest(&self, profile_id: &str) -> Result<Option<ExecutionProfileView>> {
        let row = sqlx::query(
            r#"SELECT profile_id, version, name, description, supported_client_types,
                      system_prompt_template, turn_prompt_template, default_session_role,
                      default_session_description, handle_prefix,
                      expected_output_schema, artifact_contract, default_execution_policy,
                      default_review_policy, metadata, active, archived_at, archived_reason,
                      created_at, updated_at
               FROM execution_profiles
               WHERE profile_id = ? AND active = 1 AND archived_at IS NULL
               ORDER BY rowid DESC
               LIMIT 1"#,
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_execution_profile_view).transpose()
    }

    pub async fn list_versions(
        &self,
        profile_id: &str,
        include_archived: bool,
    ) -> Result<Vec<ExecutionProfileView>> {
        let rows = if include_archived {
            sqlx::query(
                r#"SELECT profile_id, version, name, description, supported_client_types,
                          system_prompt_template, turn_prompt_template, default_session_role,
                          default_session_description, handle_prefix,
                          expected_output_schema, artifact_contract, default_execution_policy,
                          default_review_policy, metadata, active, archived_at, archived_reason,
                          created_at, updated_at
                   FROM execution_profiles
                   WHERE profile_id = ?
                   ORDER BY rowid"#,
            )
            .bind(profile_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"SELECT profile_id, version, name, description, supported_client_types,
                          system_prompt_template, turn_prompt_template, default_session_role,
                          default_session_description, handle_prefix,
                          expected_output_schema, artifact_contract, default_execution_policy,
                          default_review_policy, metadata, active, archived_at, archived_reason,
                          created_at, updated_at
                   FROM execution_profiles
                   WHERE profile_id = ? AND active = 1 AND archived_at IS NULL
                   ORDER BY rowid"#,
            )
            .bind(profile_id)
            .fetch_all(&self.pool)
            .await?
        };

        rows.into_iter()
            .map(row_to_execution_profile_view)
            .collect()
    }

    pub async fn create_profile(
        &self,
        request: UpsertExecutionProfileRequest,
        idempotency_key: Option<&str>,
    ) -> Result<AgentProfileCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response("create_agent_profile", key)
                .await?
        {
            return Ok(AgentProfileCommandOutcome {
                data: response,
                duplicate: true,
            });
        }
        if self.profile_exists(&request.profile_id).await? {
            return Err(Error::StateConflict(format!(
                "execution profile {} already exists; create a new version instead",
                request.profile_id
            )));
        }
        self.create_version("create_agent_profile", request, idempotency_key)
            .await
    }

    pub async fn create_profile_version(
        &self,
        profile_id: &str,
        request: UpsertExecutionProfileRequest,
        idempotency_key: Option<&str>,
    ) -> Result<AgentProfileCommandOutcome> {
        if profile_id != request.profile_id {
            return Err(Error::Domain(format!(
                "profile_id in path ({profile_id}) must match request profile_id ({})",
                request.profile_id
            )));
        }
        self.create_version(
            &format!("create_agent_profile_version:{profile_id}"),
            request,
            idempotency_key,
        )
        .await
    }

    async fn create_version(
        &self,
        operation: &str,
        request: UpsertExecutionProfileRequest,
        idempotency_key: Option<&str>,
    ) -> Result<AgentProfileCommandOutcome> {
        validate_request(&request)?;

        if let Some(key) = idempotency_key
            && let Some(response) = self.idempotency_response(operation, key).await?
        {
            return Ok(AgentProfileCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let supported_client_types = serde_json::to_string(&request.supported_client_types)?;
        let artifact_contract = serde_json::to_string(&request.artifact_contract)?;
        let default_execution_policy = serde_json::to_string(&request.default_execution_policy)?;
        let default_review_policy = serde_json::to_string(&request.default_review_policy)?;
        let metadata = serde_json::to_string(&request.metadata)?;

        let result = sqlx::query(
            r#"INSERT INTO execution_profiles (
                    profile_id, version, name, description, supported_client_types,
                    system_prompt_template, turn_prompt_template, default_session_role,
                    default_session_description, handle_prefix,
                    expected_output_schema, artifact_contract, default_execution_policy,
                    default_review_policy, metadata
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&request.profile_id)
        .bind(&request.version)
        .bind(&request.name)
        .bind(&request.description)
        .bind(supported_client_types)
        .bind(&request.system_prompt_template)
        .bind(&request.turn_prompt_template)
        .bind(&request.default_session_role)
        .bind(&request.default_session_description)
        .bind(&request.handle_prefix)
        .bind(&request.expected_output_schema)
        .bind(artifact_contract)
        .bind(default_execution_policy)
        .bind(default_review_policy)
        .bind(metadata)
        .execute(&self.pool)
        .await;

        if let Err(error) = result {
            if is_unique_constraint(&error) {
                return Err(Error::StateConflict(format!(
                    "execution profile {} version {} already exists",
                    request.profile_id, request.version
                )));
            }
            return Err(error.into());
        }

        let profile = self
            .get_version(&request.profile_id, &request.version)
            .await?
            .ok_or_else(|| Error::Domain("created execution profile missing".to_string()))?;
        let data = json!({ "agent_profile": profile });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(operation, key, &data)
                .await?;
        }
        Ok(AgentProfileCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub(crate) async fn get_version(
        &self,
        profile_id: &str,
        version: &str,
    ) -> Result<Option<ExecutionProfileView>> {
        let row = sqlx::query(
            r#"SELECT profile_id, version, name, description, supported_client_types,
                      system_prompt_template, turn_prompt_template, default_session_role,
                      default_session_description, handle_prefix,
                      expected_output_schema, artifact_contract, default_execution_policy,
                      default_review_policy, metadata, active, archived_at, archived_reason,
                      created_at, updated_at
               FROM execution_profiles
               WHERE profile_id = ? AND version = ?"#,
        )
        .bind(profile_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_execution_profile_view).transpose()
    }

    pub async fn update_version(
        &self,
        profile_id: &str,
        version: &str,
        request: UpsertExecutionProfileRequest,
        idempotency_key: Option<&str>,
    ) -> Result<AgentProfileCommandOutcome> {
        if profile_id != request.profile_id || version != request.version {
            return Err(Error::Domain(
                "profile_id and version in path must match request body".to_string(),
            ));
        }
        validate_request(&request)?;
        let operation = format!("update_agent_profile_version:{profile_id}:{version}");
        if let Some(key) = idempotency_key
            && let Some(response) = self.idempotency_response(&operation, key).await?
        {
            return Ok(AgentProfileCommandOutcome {
                data: response,
                duplicate: true,
            });
        }
        let current = self
            .get_version(profile_id, version)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!("agent profile {profile_id}@{version} not found"))
            })?;
        ensure_not_builtin(&current)?;

        let supported_client_types = serde_json::to_string(&request.supported_client_types)?;
        let artifact_contract = serde_json::to_string(&request.artifact_contract)?;
        let default_execution_policy = serde_json::to_string(&request.default_execution_policy)?;
        let default_review_policy = serde_json::to_string(&request.default_review_policy)?;
        let metadata = serde_json::to_string(&request.metadata)?;

        sqlx::query(
            r#"UPDATE execution_profiles
               SET name = ?, description = ?, supported_client_types = ?,
                   system_prompt_template = ?, turn_prompt_template = ?, default_session_role = ?,
                   default_session_description = ?, handle_prefix = ?, expected_output_schema = ?,
                   artifact_contract = ?, default_execution_policy = ?, default_review_policy = ?,
                   metadata = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE profile_id = ? AND version = ?"#,
        )
        .bind(&request.name)
        .bind(&request.description)
        .bind(supported_client_types)
        .bind(&request.system_prompt_template)
        .bind(&request.turn_prompt_template)
        .bind(&request.default_session_role)
        .bind(&request.default_session_description)
        .bind(&request.handle_prefix)
        .bind(&request.expected_output_schema)
        .bind(artifact_contract)
        .bind(default_execution_policy)
        .bind(default_review_policy)
        .bind(metadata)
        .bind(profile_id)
        .bind(version)
        .execute(&self.pool)
        .await?;

        let profile = self
            .get_version(profile_id, version)
            .await?
            .ok_or_else(|| Error::Domain("updated execution profile missing".to_string()))?;
        let data = json!({ "agent_profile": profile });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&operation, key, &data)
                .await?;
        }
        Ok(AgentProfileCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn archive_version(
        &self,
        profile_id: &str,
        version: &str,
        idempotency_key: Option<&str>,
    ) -> Result<AgentProfileCommandOutcome> {
        let operation = format!("archive_agent_profile_version:{profile_id}:{version}");
        if let Some(key) = idempotency_key
            && let Some(response) = self.idempotency_response(&operation, key).await?
        {
            return Ok(AgentProfileCommandOutcome {
                data: response,
                duplicate: true,
            });
        }
        let current = self
            .get_version(profile_id, version)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!("agent profile {profile_id}@{version} not found"))
            })?;
        ensure_not_builtin(&current)?;
        if current.active {
            sqlx::query(
                r#"UPDATE execution_profiles
                   SET active = 0,
                       archived_at = COALESCE(archived_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                       archived_reason = COALESCE(archived_reason, 'deleted via External API'),
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE profile_id = ? AND version = ?"#,
            )
            .bind(profile_id)
            .bind(version)
            .execute(&self.pool)
            .await?;
        }
        let profile = self
            .get_version(profile_id, version)
            .await?
            .ok_or_else(|| Error::Domain("archived execution profile missing".to_string()))?;
        let data = json!({ "agent_profile": profile });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&operation, key, &data)
                .await?;
        }
        Ok(AgentProfileCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn archive_profile(
        &self,
        profile_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<AgentProfileCommandOutcome> {
        let operation = format!("archive_agent_profile:{profile_id}");
        if let Some(key) = idempotency_key
            && let Some(response) = self.idempotency_response(&operation, key).await?
        {
            return Ok(AgentProfileCommandOutcome {
                data: response,
                duplicate: true,
            });
        }
        let versions = self.list_versions(profile_id, true).await?;
        if versions.is_empty() {
            return Err(Error::NotFound(format!(
                "agent profile {profile_id} not found"
            )));
        }
        for version in &versions {
            ensure_not_builtin(version)?;
        }
        let result = sqlx::query(
            r#"UPDATE execution_profiles
               SET active = 0,
                   archived_at = COALESCE(archived_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                   archived_reason = COALESCE(archived_reason, 'deleted via External API'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE profile_id = ? AND active = 1"#,
        )
        .bind(profile_id)
        .execute(&self.pool)
        .await?;
        let data = json!({ "profile_id": profile_id, "archived_versions": result.rows_affected() });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&operation, key, &data)
                .await?;
        }
        Ok(AgentProfileCommandOutcome {
            data,
            duplicate: false,
        })
    }

    async fn profile_exists(&self, profile_id: &str) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM execution_profiles WHERE profile_id = ?)",
        )
        .bind(profile_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists != 0)
    }

    async fn idempotency_response(&self, operation: &str, key: &str) -> Result<Option<Value>> {
        let row =
            sqlx::query("SELECT response FROM idempotency_keys WHERE operation = ? AND key = ?")
                .bind(operation)
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;

        row.map(|row| {
            let response: String = row.try_get("response")?;
            Ok(serde_json::from_str(&response)?)
        })
        .transpose()
    }

    async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO idempotency_keys (operation, key, response)
               VALUES (?, ?, ?)
               ON CONFLICT(operation, key) DO NOTHING"#,
        )
        .bind(operation)
        .bind(key)
        .bind(serde_json::to_string(response)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn validate_request(request: &UpsertExecutionProfileRequest) -> Result<()> {
    validate_non_empty("profile_id", &request.profile_id)?;
    validate_non_empty("version", &request.version)?;
    validate_non_empty("name", &request.name)?;
    for client_type in &request.supported_client_types {
        if !is_supported_client_type(client_type) {
            return Err(Error::Domain(format!(
                "unsupported client_type in supported_client_types: {client_type}"
            )));
        }
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Domain(format!("{field} cannot be empty")));
    }
    Ok(())
}

fn is_unique_constraint(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(database_error)
            if database_error.code().as_deref() == Some("1555")
                || database_error.message().contains("UNIQUE constraint failed")
    )
}

fn ensure_not_builtin(profile: &ExecutionProfileView) -> Result<()> {
    if profile
        .metadata
        .get("builtin")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(Error::StateConflict(format!(
            "builtin execution profile {} cannot be modified or deleted",
            profile.profile_id
        )));
    }
    Ok(())
}

fn row_to_execution_profile_view(row: sqlx::sqlite::SqliteRow) -> Result<ExecutionProfileView> {
    Ok(ExecutionProfileView {
        profile_id: row.try_get("profile_id")?,
        version: row.try_get("version")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        supported_client_types: json_field(&row, "supported_client_types")?,
        system_prompt_template: row.try_get("system_prompt_template")?,
        turn_prompt_template: row.try_get("turn_prompt_template")?,
        default_session_role: row.try_get("default_session_role")?,
        default_session_description: row.try_get("default_session_description")?,
        handle_prefix: row.try_get("handle_prefix")?,
        expected_output_schema: row.try_get("expected_output_schema")?,
        artifact_contract: json_field(&row, "artifact_contract")?,
        default_execution_policy: json_field(&row, "default_execution_policy")?,
        default_review_policy: json_field(&row, "default_review_policy")?,
        metadata: json_field(&row, "metadata")?,
        active: row.try_get::<i64, _>("active")? != 0,
        archived_at: row.try_get("archived_at")?,
        archived_reason: row.try_get("archived_reason")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn json_field<T>(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let raw: String = row.try_get(column)?;
    Ok(serde_json::from_str(&raw)?)
}
