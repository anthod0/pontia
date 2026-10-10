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
        events: Vec<DomainEvent>,
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
                let spec = self.clients.spec(&event.client_type);
                ensure_runtime_fence_in_tx(
                    &mut tx,
                    &event,
                    spec.is_some_and(|spec| spec.adapter.native_turn_identity),
                    spec.is_some_and(|spec| {
                        spec.adapter.runtime_binding.requires_session_runtime()
                    }),
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
        if matches!(
            event.event_type,
            EventType::RuntimeExited | EventType::SessionExited
        ) && let Some(fingerprint) = event.payload["process_fingerprint"].as_str()
        {
            let current: Option<String> = sqlx::query_scalar("SELECT process_fingerprint FROM session_runtimes WHERE runtime_id = ? AND session_id = ?")
                .bind(event.payload["runtime_id"].as_str()).bind(&event.session_id).fetch_optional(&mut *tx).await?.flatten();
            if current.as_deref() != Some(fingerprint) {
                return Err(Error::StateConflict(
                    "Process observation has been replaced".into(),
                ));
            }
        }
        if let Some(startup_event_id) = event.payload["startup_event_id"].as_str() {
            let latest: Option<String> = sqlx::query_scalar("SELECT event_id FROM events WHERE session_id=? AND event_type IN ('session.starting','session.resuming') ORDER BY rowid DESC LIMIT 1")
                .bind(&event.session_id).fetch_optional(&mut *tx).await?;
            if latest.as_deref() != Some(startup_event_id) {
                return Ok(CommitOutcome::Skipped);
            }
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
        let lifecycle = self.clients.spec(&event.client_type).map_or(
            crate::client_contract::SessionLifecycleBehavior::DEFAULT,
            |spec| spec.adapter.lifecycle,
        );
        if lifecycle.coupled_runtime
            && event.event_type == EventType::SessionReady
            && sessions
                .first()
                .is_some_and(|session| session.state.is_terminal())
        {
            return Err(Error::StateConflict(
                "Terminal runtime-coupled session cannot become ready".into(),
            ));
        }
        if lifecycle.coupled_runtime
            && event.event_type == EventType::SessionExited
            && sessions
                .first()
                .is_some_and(|session| session.state == SessionState::Exited)
        {
            let existing: Option<String> = sqlx::query_scalar("SELECT event_id FROM events WHERE session_id=? AND event_type='session.exited' ORDER BY rowid DESC LIMIT 1")
                .bind(&event.session_id).fetch_optional(&mut *tx).await?;
            if let Some(existing) = existing {
                return duplicate_without_effects(&mut tx, event, existing).await;
            }
        }
        if event.event_type == EventType::RuntimeExited {
            let runtime_id = event.payload["runtime_id"]
                .as_str()
                .expect("runtime event shape was validated");
            let role = event.payload["role"].as_str().unwrap_or("tui");
            let already_exited: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM session_runtimes WHERE runtime_id=? AND session_id=? AND role=? AND state='exited')",
            )
            .bind(runtime_id)
            .bind(&event.session_id)
            .bind(role)
            .fetch_one(&mut *tx)
            .await?;
            if already_exited {
                let existing: Option<String> = sqlx::query_scalar(
                    "SELECT event_id FROM events WHERE session_id=? AND event_type='runtime.exited' AND json_extract(payload,'$.runtime_id')=? ORDER BY rowid DESC LIMIT 1",
                )
                .bind(&event.session_id)
                .bind(runtime_id)
                .fetch_optional(&mut *tx)
                .await?;
                if let Some(existing) = existing {
                    return duplicate_without_effects(&mut tx, event, existing).await;
                }
            }
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
        let mut projection = ProjectionState::with_existing(sessions, turns)
            .with_execution_lifetime(lifecycle.execution_lifetime);
        projection.apply(&event)?;
        let runtime_event = self
            .clients
            .get(&event.client_type)
            .and_then(|client| client.events.as_ref())
            .map(|interpreter| interpreter.accompanying_runtime_event(&event))
            .transpose()?
            .flatten();
        if let Some(runtime_event) = &runtime_event {
            projection.apply(runtime_event)?;
        }

        project_runtime_in_tx(&mut tx, &event).await?;
        insert_event_in_tx(&mut tx, &event).await?;
        if let Some(runtime_event) = &runtime_event {
            project_runtime_in_tx(&mut tx, runtime_event).await?;
            insert_event_in_tx(&mut tx, runtime_event).await?;
        }

        let state_version =
            SqliteEventRepository::session_event_count_in_tx(&mut tx, &event.session_id).await?;

        persist_projections_in_tx(&mut tx, &projection, state_version).await?;

        if let Some(binding) = initial_agent_binding {
            crate::sessions::upsert_agent_binding_in_tx(&mut tx, binding).await?;
        }

        crate::sessions::register_agent_binding_for_ready_event_in_tx(&mut tx, &event).await?;

        tx.commit().await?;

        let mut events = vec![event.clone()];
        if let Some(runtime_event) = runtime_event {
            events.push(runtime_event);
        }
        Ok(CommitOutcome::Committed {
            events,
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

async fn project_runtime_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &DomainEvent,
) -> Result<()> {
    let state = match event.event_type {
        EventType::RuntimeStarting => "starting",
        EventType::RuntimeReady => "running",
        EventType::RuntimeExited => "exited",
        _ => return Ok(()),
    };
    let runtime_id = event.payload["runtime_id"]
        .as_str()
        .expect("runtime event shape was validated");
    let role = event.payload["role"].as_str().unwrap_or("tui");
    if matches!(
        event.event_type,
        EventType::RuntimeStarting | EventType::RuntimeReady
    ) {
        sqlx::query("INSERT INTO session_runtimes(runtime_id, session_id, role, state, created_at) VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) ON CONFLICT(runtime_id) DO NOTHING")
            .bind(runtime_id)
            .bind(&event.session_id)
            .bind(role)
            .bind(state)
            .execute(&mut **tx)
            .await?;
    }
    if event.event_type == EventType::RuntimeStarting {
        sqlx::query(
            r#"UPDATE session_runtimes
               SET start_command = COALESCE(?, start_command),
                   tmux_socket_path = COALESCE(?, tmux_socket_path),
                   tmux_pane_id = COALESCE(?, tmux_pane_id),
                   process_fingerprint = ?
               WHERE runtime_id = ? AND session_id = ? AND role = ?"#,
        )
        .bind(event.payload["start_command"].as_str())
        .bind(event.payload["tmux_socket_path"].as_str())
        .bind(event.payload["tmux_pane_id"].as_str())
        .bind(event.payload["process_fingerprint"].as_str())
        .bind(runtime_id)
        .bind(&event.session_id)
        .bind(role)
        .execute(&mut **tx)
        .await?;
    }
    let allowed_prior_states = match event.event_type {
        EventType::RuntimeReady => " AND state IN ('starting', 'running')",
        EventType::RuntimeStarting | EventType::RuntimeExited => "",
        _ => unreachable!(),
    };
    let updated = sqlx::query(&format!(
        "UPDATE session_runtimes SET state = ? WHERE runtime_id = ? AND session_id = ? AND role = ?{allowed_prior_states}"
    ))
    .bind(state)
    .bind(runtime_id)
    .bind(&event.session_id)
    .bind(role)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(Error::StateConflict(
            "Runtime lifecycle target is missing".into(),
        ));
    }
    Ok(())
}

async fn duplicate_without_effects(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: DomainEvent,
    event_id: String,
) -> Result<CommitOutcome> {
    let state_version =
        SqliteEventRepository::session_event_count_in_tx(tx, &event.session_id).await?;
    Ok(CommitOutcome::Duplicate {
        cleanup: None,
        result: EventIngestResult {
            accepted: true,
            duplicate: true,
            event_id,
            session_id: event.session_id,
            turn_id: event.turn_id,
            state_version,
        },
    })
}
