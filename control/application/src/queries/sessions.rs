use pontia_agent_clients as agent_clients;
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
        let repository = SqliteSessionRepository::new(self.pool.clone());
        let row = repository
            .get_runtime_binding_capabilities(&session.session_id)
            .await?;

        if let Some(row) = row {
            let capabilities: SessionCapabilities = serde_json::from_str(&row.capabilities)?;
            session.capabilities = if agent_clients::get_client_spec(&session.client_type)
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
            };
        }

        session.lineage = self.session_lineage(&session.session_id).await?;

        Ok(())
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
