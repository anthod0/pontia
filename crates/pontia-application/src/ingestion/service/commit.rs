use super::{
    enrichment::{enrich_timeline_boundary, enrich_topology, should_resolve_topology},
    persistence::{insert_event_in_tx, persist_projections_in_tx},
    reporting_failure,
    validation::{ensure_runtime_fence_in_tx, validate_turn_identity_in_tx},
};
use crate::ingestion::projection_rows::{session_from_row, turn_from_row};
use crate::{EventIngestResult, UpsertAgentBindingRequest, clients::ClientRegistry};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType, MAX_TURN_INPUT_SUMMARY_CHARS, ProjectionState, SessionState},
};
use pontia_storage_sqlite::repositories::{
    events::SqliteEventRepository, sessions::SqliteSessionRepository, turns::SqliteTurnRepository,
};
use sqlx::SqlitePool;

pub(super) enum CommitOutcome {
    Skipped,
    Duplicate {
        result: EventIngestResult,
        cleanup: Option<DomainEvent>,
    },
    Committed {
        result: EventIngestResult,
        event: DomainEvent,
    },
}

pub(super) struct EventCommitter {
    pool: SqlitePool,
    clients: ClientRegistry,
}

impl EventCommitter {
    pub(super) fn new(pool: SqlitePool, clients: ClientRegistry) -> Self {
        Self { pool, clients }
    }
    pub(super) async fn commit(
        &self,
        mut event: DomainEvent,
        initial_agent_binding: Option<UpsertAgentBindingRequest>,
        enforce_runtime_fence: bool,
        expected_session_state: Option<SessionState>,
    ) -> Result<CommitOutcome> {
        // Bound durable input at the shared ingestion boundary, including
        // Pontia-owned and in-process events that do not pass through HTTP.
        if event.event_type.is_turn_event() {
            for pointer in ["/input/summary", "/input_summary"] {
                if let Some(serde_json::Value::String(summary)) = event.payload.pointer_mut(pointer)
                {
                    *summary = summary.chars().take(MAX_TURN_INPUT_SUMMARY_CHARS).collect();
                }
            }
        }
        if event.event_type.is_turn_event() && event.turn_id.is_none() {
            return Err(Error::Domain(format!(
                "{} must carry turn_id",
                event.event_type
            )));
        }
        if let Some(existing_version) = self
            .existing_event_state_version(&event.event_id, &event.session_id)
            .await?
        {
            return Ok(CommitOutcome::Duplicate {
                cleanup: Some(event.clone()),
                result: EventIngestResult {
                    accepted: true,
                    duplicate: true,
                    event_id: event.event_id,
                    session_id: event.session_id,
                    turn_id: event.turn_id,
                    state_version: existing_version,
                },
            });
        }

        let evidence = if let Some(data) = self.clients.data(&event.client_type) {
            data.take_evidence(&mut event)
        } else {
            crate::clients::NativeEventEvidence {
                entry_anchor: event
                    .payload
                    .get("native_turn_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                topology: None,
            }
        };
        enrich_timeline_boundary(&self.pool, &self.clients, &mut event, evidence.entry_anchor)
            .await;
        let topology_evidence = evidence.topology;
        let topology_binding_id = if should_resolve_topology(&self.clients, &event) {
            crate::AgentBindingService::new(self.pool.clone())
                .binding_for_session(&event.session_id)
                .await
                .ok()
                .flatten()
                .map(|binding| binding.id)
        } else {
            None
        };

        let mut tx = self.pool.begin().await?;
        if event.event_type != EventType::SessionCreated {
            let session_exists =
                SqliteTurnRepository::serialize_session_turn_writes_if_exists_in_tx(
                    &mut tx,
                    &event.session_id,
                )
                .await?;
            if !session_exists && (event.event_type.is_turn_event() || enforce_runtime_fence) {
                SqliteTurnRepository::serialize_session_turn_writes_in_tx(
                    &mut tx,
                    &event.session_id,
                )
                .await?;
            }
            if enforce_runtime_fence {
                ensure_runtime_fence_in_tx(
                    &mut tx,
                    &event,
                    self.clients
                        .spec(&event.client_type)
                        .is_some_and(|spec| spec.adapter.native_turn_identity),
                )
                .await?;
            }
        }
        if let Some(event_id) =
            reporting_failure::existing_reporting_failure_in_tx(&mut tx, &event).await?
        {
            let state_version =
                SqliteEventRepository::session_event_count_in_tx(&mut tx, &event.session_id)
                    .await?;
            tx.commit().await?;
            return Ok(CommitOutcome::Duplicate {
                cleanup: None,
                result: EventIngestResult {
                    accepted: true,
                    duplicate: true,
                    event_id,
                    session_id: event.session_id,
                    turn_id: None,
                    state_version,
                },
            });
        }
        if sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE event_id=?")
            .bind(&event.event_id)
            .fetch_one(&mut *tx)
            .await?
            > 0
        {
            let state_version =
                SqliteEventRepository::session_event_count_in_tx(&mut tx, &event.session_id)
                    .await?;
            tx.commit().await?;
            return Ok(CommitOutcome::Duplicate {
                cleanup: None,
                result: EventIngestResult {
                    accepted: true,
                    duplicate: true,
                    event_id: event.event_id,
                    session_id: event.session_id,
                    turn_id: event.turn_id,
                    state_version,
                },
            });
        }
        validate_turn_identity_in_tx(&mut tx, &event, enforce_runtime_fence).await?;
        let sessions =
            SqliteSessionRepository::load_projection_rows_in_tx(&mut tx, &event.session_id)
                .await?
                .into_iter()
                .map(session_from_row)
                .collect::<Result<Vec<_>>>()?;
        if let Some(expected_state) = expected_session_state
            && !sessions
                .first()
                .is_some_and(|session| session.state == expected_state)
        {
            return Ok(CommitOutcome::Skipped);
        }
        let turns = SqliteTurnRepository::load_projection_rows_in_tx(&mut tx, &event.session_id)
            .await?
            .into_iter()
            .map(turn_from_row)
            .collect::<Result<Vec<_>>>()?;
        enrich_topology(
            &self.clients,
            &mut event,
            topology_binding_id,
            topology_evidence,
            &turns,
        );
        let mut projection = ProjectionState::with_existing(sessions, turns);
        projection.apply(&event)?;

        insert_event_in_tx(&mut tx, &event).await?;

        let state_version =
            SqliteEventRepository::session_event_count_in_tx(&mut tx, &event.session_id).await?;

        if event.event_type != EventType::SessionMessageUpdated {
            persist_projections_in_tx(&mut tx, &projection, state_version).await?;
        }

        if let Some(binding) = initial_agent_binding {
            crate::sessions::upsert_agent_binding_in_tx(&mut tx, binding).await?;
        }

        crate::sessions::register_agent_binding_for_ready_event_in_tx(&mut tx, &event).await?;

        tx.commit().await?;

        Ok(CommitOutcome::Committed {
            event: event.clone(),
            result: EventIngestResult {
                accepted: true,
                duplicate: false,
                event_id: event.event_id,
                session_id: event.session_id,
                turn_id: event.turn_id,
                state_version,
            },
        })
    }

    async fn existing_event_state_version(
        &self,
        event_id: &str,
        session_id: &str,
    ) -> Result<Option<i64>> {
        SqliteEventRepository::new(self.pool.clone())
            .existing_event_state_version(event_id, session_id)
            .await
    }
}
