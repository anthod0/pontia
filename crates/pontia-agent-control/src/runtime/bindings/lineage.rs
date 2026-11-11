use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::{
    agent_bindings::SqliteAgentBindingRepository, sessions::SqliteSessionRepository,
};
use serde_json::json;

use super::{
    RuntimeBindingUpsertRequest, request::non_empty, service::RuntimeBindingUpsertService,
};

impl RuntimeBindingUpsertService {
    pub(super) async fn upsert_fork_lineage(
        &self,
        child_session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<()> {
        let parent_session_id = self.resolve_parent_session_id(request).await?.ok_or_else(|| {
            Error::Domain(
                "fork runtime binding upsert requires parent_session_id or parent_client_session_key"
                    .to_string(),
            )
        })?;
        if parent_session_id == child_session_id {
            return Err(Error::Domain(
                "fork child session cannot be the same as parent session".to_string(),
            ));
        }
        if !SqliteSessionRepository::new(self.pool.clone())
            .exists(&parent_session_id)
            .await?
        {
            return Err(Error::NotFound(format!(
                "parent session {parent_session_id} not found"
            )));
        }
        let parent_client_session_key =
            match non_empty(request.parent_client_session_key.as_deref()) {
                Some(key) => Some(key),
                None => {
                    SqliteAgentBindingRepository::new(self.pool.clone())
                        .client_session_key_for_session(&parent_session_id, &request.client_type)
                        .await?
                }
            };
        let metadata = if request.lineage_metadata.is_null() {
            json!({})
        } else {
            request.lineage_metadata.clone()
        };
        sqlx::query(
            r#"INSERT INTO session_lineage
               (child_session_id, parent_session_id, relation_type, forked_from_turn_id,
                forked_from_client_node_id, parent_client_session_key, child_client_session_key,
                metadata)
               VALUES (?, ?, 'fork', ?, ?, ?, ?, ?)
               ON CONFLICT(child_session_id) DO UPDATE SET
                   parent_session_id = excluded.parent_session_id,
                   relation_type = excluded.relation_type,
                   forked_from_turn_id = excluded.forked_from_turn_id,
                   forked_from_client_node_id = excluded.forked_from_client_node_id,
                   parent_client_session_key = excluded.parent_client_session_key,
                   child_client_session_key = excluded.child_client_session_key,
                   metadata = excluded.metadata"#,
        )
        .bind(child_session_id)
        .bind(parent_session_id)
        .bind(non_empty(request.forked_from_turn_id.as_deref()))
        .bind(non_empty(request.forked_from_client_node_id.as_deref()))
        .bind(parent_client_session_key)
        .bind(non_empty(Some(&request.client_session_key)))
        .bind(serde_json::to_string(&metadata)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn resolve_parent_session_id(
        &self,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<Option<String>> {
        if let Some(parent_session_id) = non_empty(request.parent_session_id.as_deref()) {
            return Ok(Some(parent_session_id));
        }
        if let Some(parent_client_session_key) =
            non_empty(request.parent_client_session_key.as_deref())
        {
            return self
                .session_id_for_client_session(&request.client_type, &parent_client_session_key)
                .await;
        }
        Ok(None)
    }
}
