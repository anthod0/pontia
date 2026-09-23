use serde_json::Value;
use sqlx::SqlitePool;

use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    error::{Error, Result},
    ids::{new_event_id, new_turn_id},
};
use pontia_storage_sqlite::repositories::{
    sessions::SqliteSessionRepository, turns::SqliteTurnRepository,
};

/// A client-observed fact at the application ingestion boundary.
///
/// Pontia identity, source, client type, canonical time and canonical payload are
/// deliberately absent: those belong to normalization, not to the client.
#[derive(Debug, Clone)]
pub struct ReportedFact {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub fact_type: EventType,
    pub data: Value,
}

#[derive(Clone)]
pub struct EventReportNormalizer {
    clients: crate::clients::ClientRegistry,
    pool: SqlitePool,
}

impl EventReportNormalizer {
    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            clients: Default::default(),
        }
    }

    pub async fn normalize(&self, mut fact: ReportedFact) -> Result<ReportedEvent> {
        if !fact.fact_type.is_client_reportable() {
            return Err(Error::Domain(format!(
                "{} is owned by the Pontia control plane and cannot be reported by an agent client",
                fact.fact_type
            )));
        }

        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(&fact.session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {} not found", fact.session_id)))?;

        if event_type_can_create_turn(fact.fact_type)
            && let Some(turn_id) = fact.turn_id.as_deref()
        {
            let owning_session_id = SqliteTurnRepository::new(self.pool.clone())
                .turn_session_id(turn_id)
                .await?
                .ok_or_else(|| {
                    Error::Domain(format!(
                        "event {} cannot create client-supplied turn {turn_id}",
                        fact.fact_type
                    ))
                })?;
            if owning_session_id != fact.session_id {
                return Err(Error::Domain(format!(
                    "turn {turn_id} belongs to session {owning_session_id}, not {}",
                    fact.session_id
                )));
            }
        }

        let has_native_turn = self
            .clients
            .spec(&session.client_type)
            .is_some_and(|spec| spec.adapter.native_turn_identity)
            && fact.fact_type.is_turn_event();
        if has_native_turn && let Some(data) = self.clients.data(&session.client_type) {
            fact.data = data.normalize_payload(fact.fact_type, fact.data)?;
        }
        let native_turn_id = if has_native_turn {
            Some(crate::native_turns::native_turn_identity(&self.pool, &fact).await?)
        } else {
            None
        };
        if native_turn_id.is_some() {
            fact.turn_id = native_turn_id;
        }
        let turn_id = match (fact.fact_type, fact.turn_id) {
            (EventType::TurnStarted, None) => Some(new_turn_id().to_string()),
            (event_type, None) if event_type.requires_turn_id() => {
                return Err(Error::Domain(format!(
                    "event {event_type} requires turn_id"
                )));
            }
            (_, turn_id) => turn_id,
        };
        let source = if fact.fact_type.is_turn_event() {
            EventSource::AgentAdapter
        } else {
            EventSource::AgentClient
        };
        let payload = if has_native_turn {
            fact.data
        } else {
            match self.clients.data(&session.client_type) {
                Some(data) => data.normalize_payload(fact.fact_type, fact.data)?,
                None => fact.data,
            }
        };

        let event_id = if has_native_turn {
            format!(
                "evt_{}_{}_{}",
                session.client_type,
                turn_id.as_deref().unwrap(),
                fact.fact_type
            )
        } else {
            new_event_id().to_string()
        };
        Ok(ReportedEvent::new(
            event_id,
            fact.session_id,
            turn_id,
            source,
            session.client_type,
            fact.fact_type,
            payload,
        ))
    }
}

fn event_type_can_create_turn(event_type: EventType) -> bool {
    matches!(
        event_type,
        EventType::TurnCreated | EventType::TurnQueued | EventType::TurnStarted
    )
}
