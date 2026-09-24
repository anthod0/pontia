use pontia_core::error::Result;
use pontia_storage_sqlite::repositories::sessions::{SessionListOptions, SqliteSessionRepository};
use sqlx::Row;

use super::ExternalQueryService;
use crate::views::sessions::{SessionCapabilities, SessionLineageView, SessionView, row_to_view};

impl ExternalQueryService {
    pub async fn list_sessions(
        &self,
        include_archived: bool,
        limit: Option<u32>,
        include_pinned: bool,
    ) -> Result<Vec<SessionView>> {
        let repository = SqliteSessionRepository::new(self.pool.clone());
        let rows = repository
            .list_sessions_with_options(SessionListOptions {
                include_archived,
                limit,
                include_pinned,
            })
            .await?;

        let mut sessions = rows
            .into_iter()
            .map(row_to_view)
            .collect::<Result<Vec<_>>>()?;
        for session in &mut sessions {
            self.enrich_session_view(session).await?;
        }
        Ok(sessions)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionView>> {
        let repository = SqliteSessionRepository::new(self.pool.clone());
        let Some(row) = repository.get_session(session_id).await? else {
            return Ok(None);
        };
        let mut session = row_to_view(row)?;
        self.enrich_session_view(&mut session).await?;
        Ok(Some(session))
    }

    async fn enrich_session_view(&self, session: &mut SessionView) -> Result<()> {
        session.capabilities = self
            .session_capabilities(&session.session_id, &session.client_type)
            .await?;

        if let Some(data) = self.clients.data(&session.client_type)
            && let Some(binding) = crate::AgentBindingService::new(self.pool.clone())
                .binding_for_session(&session.session_id)
                .await?
            && let Some(result) = data.probe_timeline(
                &crate::client_contract::raw_transcripts::AgentBindingResolveRequest {
                    id: binding.id,
                    session_id: binding.session_id,
                    client_type: binding.client_type,
                    client_session_key: binding.client_session_key,
                    client_session_file: binding.client_session_file.map(Into::into),
                },
            )
        {
            session.capabilities.timeline = result.is_ok();
            session.timeline_unavailable_reason = result.err().map(|error| match error {
                pontia_core::Error::Conflict { code: "timeline_source_identity_mismatch", .. } => "Native history belongs to a different session.".into(),
                pontia_core::Error::CapabilityUnavailable(message) if message.contains("source_unavailable:") => "Native history is not available on disk yet. Retry when the source is available.".into(),
                pontia_core::Error::Conflict { code: "timeline_pending", .. } => "Native history is still being written. Retry shortly.".into(),
                _ => "The native history format is unsupported or invalid.".into(),
            });
        }
        session.lineage = self.session_lineage(&session.session_id).await?;
        if let Some(client) = self
            .clients
            .get(&session.client_type)
            .and_then(|entry| entry.session.as_ref())
        {
            let details = client
                .details(self.pool.clone(), &session.session_id)
                .await?;
            session.model_control_unavailable_reason = details.model_control_unavailable_reason;
            session
                .client_details
                .insert(session.client_type.clone(), details.data);
        }

        Ok(())
    }

    pub(super) async fn session_capabilities(
        &self,
        session_id: &str,
        client_type: &str,
    ) -> Result<SessionCapabilities> {
        let row = SqliteSessionRepository::new(self.pool.clone())
            .get_runtime_binding_capabilities(session_id)
            .await?;
        let Some(row) = row else {
            return Ok(SessionCapabilities::default());
        };
        let capabilities: SessionCapabilities = serde_json::from_str(&row.capabilities)?;
        Ok(
            if self
                .clients
                .spec(client_type)
                .and_then(|spec| spec.tmux_runtime())
                .is_some()
            {
                crate::runtime::bindings::writable_capabilities(
                    capabilities,
                    row.tmux_socket_path
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
                        && row
                            .tmux_pane_id
                            .as_deref()
                            .is_some_and(|value| !value.trim().is_empty()),
                )
            } else {
                capabilities
            },
        )
    }

    async fn session_lineage(&self, session_id: &str) -> Result<Option<SessionLineageView>> {
        let row = sqlx::query(
            r#"SELECT relation_type, parent_session_id, forked_from_turn_id,
                      forked_from_client_node_id, parent_client_session_key,
                      child_client_session_key, created_at
               FROM session_lineage
               WHERE child_session_id = ?"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            Ok(SessionLineageView {
                relation_type: row.try_get("relation_type")?,
                parent_session_id: row.try_get("parent_session_id")?,
                forked_from_turn_id: row.try_get("forked_from_turn_id")?,
                forked_from_client_node_id: row.try_get("forked_from_client_node_id")?,
                parent_client_session_key: row.try_get("parent_client_session_key")?,
                child_client_session_key: row.try_get("child_client_session_key")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .transpose()
    }
}
