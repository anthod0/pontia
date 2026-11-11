use super::ControlQueryService;
use crate::views::sessions::{SessionCapabilities, SessionLineageView, SessionView, row_to_view};
use pontia_core::Result;
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;
use sqlx::Row;
impl ControlQueryService {
    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionView>> {
        let repository = SqliteSessionRepository::new(self.pool.clone());
        let Some(row) = repository.get_session(session_id).await? else {
            return Ok(None);
        };
        self.session_view(row).await.map(Some)
    }

    pub async fn session_view(
        &self,
        row: pontia_storage_sqlite::models::sessions::SessionRow,
    ) -> Result<SessionView> {
        let mut session = row_to_view(row)?;
        self.enrich_session_view(&mut session).await?;
        Ok(session)
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
        session.runtimes = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(self.pool.clone()).list(&session.session_id).await?;
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
        if self.clients.spec(client_type).is_none() {
            return Ok(SessionCapabilities::default());
        }
        let repository = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(self.pool.clone());
        let runtimes = repository.list(session_id).await?;
        let entry = self.clients.get(client_type).expect("checked client");
        let mut capabilities = entry
            .in_process
            .as_ref()
            .map(|client| client.capabilities())
            .unwrap_or_else(|| entry.spec.capabilities.clone());
        if let Some(client) = &entry.session {
            capabilities = client.capabilities(session_id, &capabilities).await?;
            capabilities = capabilities.intersection(&entry.spec.capabilities);
        }
        if runtimes.is_empty()
            && entry
                .spec
                .adapter
                .runtime_binding
                .requires_session_runtime()
        {
            return Ok(SessionCapabilities {
                compaction_events: capabilities.compaction_events,
                compaction_history: capabilities.compaction_history,
                ..SessionCapabilities::default()
            });
        }
        if capabilities.manual_compaction {
            capabilities.manual_compaction = if let Some(client) = &entry.session {
                client
                    .manual_compaction_available(self.pool.clone(), session_id)
                    .await?
            } else if entry.spec.adapter.dispatch == crate::client_contract::DispatchMode::Connected
            {
                match &self.control {
                    Some(control) => match control.available(session_id).await {
                        Ok(available) => available,
                        Err(
                            pontia_core::Error::StateConflict(_)
                            | pontia_core::Error::CapabilityUnavailable(_),
                        ) => false,
                        Err(error) => return Err(error),
                    },
                    None => false,
                }
            } else {
                false
            };
        }
        if self
            .clients
            .spec(client_type)
            .and_then(|spec| spec.tmux_runtime())
            .is_some()
        {
            Ok(crate::runtime::bindings::writable_capabilities(
                capabilities,
                runtimes.iter().any(|runtime| {
                    runtime.role == "tui"
                        && runtime.tmux_socket_path.is_some()
                        && runtime.tmux_pane_id.is_some()
                }),
            ))
        } else {
            Ok(capabilities)
        }
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
