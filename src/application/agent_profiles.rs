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
    pub session_reuse_policy: String,
    pub expected_output_schema: Option<String>,
    pub artifact_contract: Value,
    pub default_execution_policy: Value,
    pub default_review_policy: Value,
    pub metadata: Value,
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
    pub session_reuse_policy: String,
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
                      default_session_description, handle_prefix, session_reuse_policy,
                      expected_output_schema, artifact_contract, default_execution_policy,
                      default_review_policy, metadata, created_at, updated_at
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
                      default_session_description, handle_prefix, session_reuse_policy,
                      expected_output_schema, artifact_contract, default_execution_policy,
                      default_review_policy, metadata, created_at, updated_at
               FROM execution_profiles
               WHERE profile_id = ?
               ORDER BY rowid DESC
               LIMIT 1"#,
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_execution_profile_view).transpose()
    }

    pub async fn create_profile(
        &self,
        request: UpsertExecutionProfileRequest,
        idempotency_key: Option<&str>,
    ) -> Result<AgentProfileCommandOutcome> {
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
                    default_session_description, handle_prefix, session_reuse_policy,
                    expected_output_schema, artifact_contract, default_execution_policy,
                    default_review_policy, metadata
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
        .bind(&request.session_reuse_policy)
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

    async fn get_version(
        &self,
        profile_id: &str,
        version: &str,
    ) -> Result<Option<ExecutionProfileView>> {
        let row = sqlx::query(
            r#"SELECT profile_id, version, name, description, supported_client_types,
                      system_prompt_template, turn_prompt_template, default_session_role,
                      default_session_description, handle_prefix, session_reuse_policy,
                      expected_output_schema, artifact_contract, default_execution_policy,
                      default_review_policy, metadata, created_at, updated_at
               FROM execution_profiles
               WHERE profile_id = ? AND version = ?"#,
        )
        .bind(profile_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_execution_profile_view).transpose()
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
    validate_non_empty("session_reuse_policy", &request.session_reuse_policy)?;
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
        session_reuse_policy: row.try_get("session_reuse_policy")?,
        expected_output_schema: row.try_get("expected_output_schema")?,
        artifact_contract: json_field(&row, "artifact_contract")?,
        default_execution_policy: json_field(&row, "default_execution_policy")?,
        default_review_policy: json_field(&row, "default_review_policy")?,
        metadata: json_field(&row, "metadata")?,
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
