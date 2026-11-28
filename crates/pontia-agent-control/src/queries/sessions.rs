use super::ControlQueryService;
use crate::client_contract::SessionCapabilityState;
use crate::views::sessions::{SessionCapabilities, SessionLineageView, SessionView, row_to_view};
use pontia_core::Result;
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;
use sqlx::Row;
impl ControlQueryService {
    fn client_store(&self) -> crate::ClientControlStore {
        crate::ClientControlStore::new(self.pool.clone())
    }

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
            .await?
            .available;

        if let Some(data) = self.clients.data(&session.client_type)
            && let Some(binding) = crate::AgentBindingService::new(self.pool.clone())
                .binding_for_session(&session.session_id)
                .await?
            && let Some(result) = data.probe_timeline(
                session.state == "exited",
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
            session.timeline_unavailable_reason = result.err().map(timeline_unavailable_reason);
        }
        session.runtimes = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(self.pool.clone()).list(&session.session_id).await?;
        if session
            .metadata
            .get("incompatible_control_runtime")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| session.runtimes.iter().any(|r| r.runtime_id == id))
        {
            session.control_connected = Some(false);
            session.control_disconnected_reason = Some("unsupported_protocol_version");
        } else if let Some(entry) = self.clients.get(&session.client_type)
            && let Some(client) = &entry.session
        {
            session.control_connected = client
                .control_connected(self.client_store(), &session.session_id)
                .await?;
        }
        session.lineage = self.session_lineage(&session.session_id).await?;
        if let Some(client) = self
            .clients
            .get(&session.client_type)
            .and_then(|entry| entry.session.as_ref())
        {
            let details = client
                .details(self.client_store(), &session.session_id)
                .await?;
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
    ) -> Result<SessionCapabilityState> {
        let Some(entry) = self.clients.get(client_type) else {
            return Ok(Default::default());
        };
        let repository = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(self.pool.clone());
        let runtimes = repository.list(session_id).await?;
        let mut state = if let Some(client) = &entry.session {
            let mut state = client
                .capability_state(session_id, &entry.spec.capabilities)
                .await?;
            state.supported = state.supported.intersection(&entry.spec.capabilities);
            state.available = state.available.intersection(&entry.spec.capabilities);
            state
        } else {
            entry.spec.capabilities.clone().into()
        };
        let capabilities = &mut state.available;
        if runtimes.is_empty()
            && entry
                .spec
                .adapter
                .runtime_binding
                .requires_session_runtime()
        {
            *capabilities = SessionCapabilities {
                timeline: capabilities.timeline
                    && self
                        .clients
                        .data(client_type)
                        .is_some_and(|data| data.timeline_uses_lifecycle_anchors()),
                compaction_events: capabilities.compaction_events,
                compaction_history: capabilities.compaction_history,
                ..SessionCapabilities::default()
            };
            return Ok(state);
        }
        if capabilities.manual_compaction {
            capabilities.manual_compaction = if let Some(client) = &entry.session {
                client
                    .manual_compaction_available(self.client_store(), session_id)
                    .await?
            } else {
                false
            };
        }
        if capabilities.reasoning_effort_control {
            capabilities.reasoning_effort_control = if let Some(client) = &entry.session {
                let available = client
                    .reasoning_effort_control_available(self.client_store(), session_id)
                    .await?;
                if !available
                    && client
                        .control_connected(self.client_store(), session_id)
                        .await?
                        == Some(true)
                {
                    state.supported.reasoning_effort_control = false;
                }
                available
            } else {
                false
            };
        }
        if entry.spec.owns_interactive_turn() {
            crate::runtime::bindings::writable_capabilities(
                capabilities,
                runtimes.iter().any(|runtime| {
                    Some(runtime.role.as_str()) == entry.spec.adapter.control_runtime_role
                }),
            );
        }
        Ok(state)
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

fn timeline_unavailable_reason(error: pontia_core::Error) -> String {
    use pontia_core::Error;

    match error {
        Error::Conflict {
            code: "timeline_source_identity_mismatch",
            ..
        } => "Native history belongs to a different session.",
        Error::Unavailable(_) => {
            "Native history is not available on disk yet. Retry when the source is available."
        }
        Error::Conflict {
            code: "timeline_pending",
            ..
        } => "Native history is still being written. Retry shortly.",
        _ => "The native history format is unsupported or invalid.",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::timeline_unavailable_reason;
    use pontia_core::Error;

    #[test]
    fn pending_history_reload_has_a_distinct_reason_from_invalid_history() {
        let reattached = Error::Conflict {
            code: "timeline_pending",
            message: "Waiting for native history".into(),
        };
        let invalid = Error::Unsupported("invalid history format".into());

        assert_ne!(
            timeline_unavailable_reason(reattached),
            timeline_unavailable_reason(invalid),
        );
    }
}
