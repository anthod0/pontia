use pontia_core::Result;
use sqlx::SqlitePool;

use crate::models::agent_profiles::ExecutionProfileRow;

const EXECUTION_PROFILE_COLUMNS: &str = r#"profile_id, version, name, description, supported_client_types, agent_kind,
          system_prompt_template, turn_prompt_template, default_session_role,
          default_session_description, handle_prefix,
          expected_output_schema, artifact_contract, default_execution_policy,
          default_review_policy, metadata, active, archived_at, archived_reason,
          created_at, updated_at"#;

#[derive(Debug, Clone)]
pub struct ExecutionProfileUpsertRecord {
    pub profile_id: String,
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    pub supported_client_types: String,
    pub agent_kind: String,
    pub system_prompt_template: Option<String>,
    pub turn_prompt_template: Option<String>,
    pub default_session_role: Option<String>,
    pub default_session_description: Option<String>,
    pub handle_prefix: Option<String>,
    pub expected_output_schema: Option<String>,
    pub artifact_contract: String,
    pub default_execution_policy: String,
    pub default_review_policy: String,
    pub metadata: String,
}

#[derive(Debug, Clone)]
pub struct ExecutionProfileUpdateRecord {
    pub profile_id: String,
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    pub supported_client_types: String,
    pub agent_kind: String,
    pub system_prompt_template: Option<String>,
    pub turn_prompt_template: Option<String>,
    pub default_session_role: Option<String>,
    pub default_session_description: Option<String>,
    pub handle_prefix: Option<String>,
    pub expected_output_schema: Option<String>,
    pub artifact_contract: String,
    pub default_execution_policy: String,
    pub default_review_policy: String,
    pub metadata: String,
}

#[derive(Debug, Clone)]
pub struct SqliteAgentProfileRepository {
    pool: SqlitePool,
}

impl SqliteAgentProfileRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert_version(&self, record: ExecutionProfileUpsertRecord) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO execution_profiles (
                    profile_id, version, name, description, supported_client_types, agent_kind,
                    system_prompt_template, turn_prompt_template, default_session_role,
                    default_session_description, handle_prefix,
                    expected_output_schema, artifact_contract, default_execution_policy,
                    default_review_policy, metadata
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(record.profile_id)
        .bind(record.version)
        .bind(record.name)
        .bind(record.description)
        .bind(record.supported_client_types)
        .bind(record.agent_kind)
        .bind(record.system_prompt_template)
        .bind(record.turn_prompt_template)
        .bind(record.default_session_role)
        .bind(record.default_session_description)
        .bind(record.handle_prefix)
        .bind(record.expected_output_schema)
        .bind(record.artifact_contract)
        .bind(record.default_execution_policy)
        .bind(record.default_review_policy)
        .bind(record.metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_version(&self, record: ExecutionProfileUpdateRecord) -> Result<()> {
        sqlx::query(
            r#"UPDATE execution_profiles
               SET name = ?, description = ?, supported_client_types = ?, agent_kind = ?,
                   system_prompt_template = ?, turn_prompt_template = ?, default_session_role = ?,
                   default_session_description = ?, handle_prefix = ?, expected_output_schema = ?,
                   artifact_contract = ?, default_execution_policy = ?, default_review_policy = ?,
                   metadata = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE profile_id = ? AND version = ?"#,
        )
        .bind(record.name)
        .bind(record.description)
        .bind(record.supported_client_types)
        .bind(record.agent_kind)
        .bind(record.system_prompt_template)
        .bind(record.turn_prompt_template)
        .bind(record.default_session_role)
        .bind(record.default_session_description)
        .bind(record.handle_prefix)
        .bind(record.expected_output_schema)
        .bind(record.artifact_contract)
        .bind(record.default_execution_policy)
        .bind(record.default_review_policy)
        .bind(record.metadata)
        .bind(record.profile_id)
        .bind(record.version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn archive_version(&self, profile_id: &str, version: &str) -> Result<()> {
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
        Ok(())
    }

    pub async fn archive_active_versions(&self, profile_id: &str) -> Result<u64> {
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
        Ok(result.rows_affected())
    }

    pub async fn list_latest(&self) -> Result<Vec<ExecutionProfileRow>> {
        let query = format!(
            r#"SELECT {EXECUTION_PROFILE_COLUMNS}
               FROM execution_profiles ep
               WHERE active = 1 AND archived_at IS NULL AND rowid = (
                   SELECT max(rowid) FROM execution_profiles latest
                   WHERE latest.profile_id = ep.profile_id
                     AND latest.active = 1
                     AND latest.archived_at IS NULL
               )
               ORDER BY profile_id"#
        );
        Ok(sqlx::query_as::<_, ExecutionProfileRow>(&query)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn list_latest_including_archived(&self) -> Result<Vec<ExecutionProfileRow>> {
        let query = format!(
            r#"SELECT {EXECUTION_PROFILE_COLUMNS}
               FROM execution_profiles ep
               WHERE rowid = (
                   SELECT max(rowid) FROM execution_profiles latest
                   WHERE latest.profile_id = ep.profile_id
               )
               ORDER BY profile_id"#
        );
        Ok(sqlx::query_as::<_, ExecutionProfileRow>(&query)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn get_latest(&self, profile_id: &str) -> Result<Option<ExecutionProfileRow>> {
        let query = format!(
            r#"SELECT {EXECUTION_PROFILE_COLUMNS}
               FROM execution_profiles
               WHERE profile_id = ? AND active = 1 AND archived_at IS NULL
               ORDER BY rowid DESC
               LIMIT 1"#
        );
        Ok(sqlx::query_as::<_, ExecutionProfileRow>(&query)
            .bind(profile_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn list_versions(
        &self,
        profile_id: &str,
        include_archived: bool,
    ) -> Result<Vec<ExecutionProfileRow>> {
        let query = if include_archived {
            format!(
                r#"SELECT {EXECUTION_PROFILE_COLUMNS}
                   FROM execution_profiles
                   WHERE profile_id = ?
                   ORDER BY rowid"#
            )
        } else {
            format!(
                r#"SELECT {EXECUTION_PROFILE_COLUMNS}
                   FROM execution_profiles
                   WHERE profile_id = ? AND active = 1 AND archived_at IS NULL
                   ORDER BY rowid"#
            )
        };
        Ok(sqlx::query_as::<_, ExecutionProfileRow>(&query)
            .bind(profile_id)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn get_version(
        &self,
        profile_id: &str,
        version: &str,
    ) -> Result<Option<ExecutionProfileRow>> {
        let query = format!(
            r#"SELECT {EXECUTION_PROFILE_COLUMNS}
               FROM execution_profiles
               WHERE profile_id = ? AND version = ?"#
        );
        Ok(sqlx::query_as::<_, ExecutionProfileRow>(&query)
            .bind(profile_id)
            .bind(version)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn profile_exists(&self, profile_id: &str) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM execution_profiles WHERE profile_id = ?)",
        )
        .bind(profile_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists != 0)
    }
}
