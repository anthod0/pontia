use pontia_core::{
    domain::TurnState,
    error::{Error, Result},
};
use pontia_storage_sqlite::repositories::{
    runtime_bindings::SqliteRuntimeBindingRepository, sessions::SqliteSessionRepository,
    turns::SqliteTurnRepository,
};
use sqlx::SqlitePool;

use super::{
    LiveOutputBatch, LiveOutputClose, LiveOutputProducer, LiveOutputPublishOutcome,
    LiveOutputSnapshot, LiveOutputSnapshotReplacement,
    store::{LiveOutputStore, LiveOutputSubscription},
    validation::{validate_identity, validate_non_empty},
};

#[derive(Clone)]
pub struct LiveOutputService {
    clients: crate::clients::ClientRegistry,
    pool: SqlitePool,
    store: LiveOutputStore,
}

impl LiveOutputService {
    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    pub(crate) fn new(pool: SqlitePool, store: LiveOutputStore) -> Self {
        Self {
            pool,
            store,
            clients: Default::default(),
        }
    }

    pub async fn publish_batch(&self, batch: LiveOutputBatch) -> Result<LiveOutputPublishOutcome> {
        self.validate_producer(&batch.producer, true).await?;
        self.store.publish_batch(batch)
    }

    pub async fn replace_snapshot(
        &self,
        replacement: LiveOutputSnapshotReplacement,
    ) -> Result<LiveOutputPublishOutcome> {
        self.validate_producer(&replacement.producer, true).await?;
        self.store.replace_snapshot(replacement)
    }

    pub async fn close(&self, close: LiveOutputClose) -> Result<LiveOutputPublishOutcome> {
        self.validate_producer(&close.producer, false).await?;
        self.store.close(close)
    }

    pub fn snapshot(&self, session_id: &str, turn_id: &str) -> Option<LiveOutputSnapshot> {
        self.store.snapshot(session_id, turn_id)
    }

    pub fn subscribe_session(&self, session_id: &str) -> LiveOutputSubscription {
        self.store.subscribe_session(session_id)
    }

    pub fn discard_turn(&self, session_id: &str, turn_id: &str) {
        self.store.discard_turn(session_id, turn_id);
    }

    pub fn discard_session(&self, session_id: &str) {
        self.store.discard_session(session_id);
    }

    async fn validate_producer(
        &self,
        producer: &LiveOutputProducer,
        require_active_turn: bool,
    ) -> Result<()> {
        let identity = &producer.identity;
        validate_identity(identity)?;
        validate_non_empty("runtime_instance_id", &producer.runtime_instance_id)?;

        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(&identity.session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {} not found", identity.session_id)))?;
        let supports_streaming = self
            .clients
            .spec(&session.client_type)
            .is_some_and(|spec| spec.capabilities.stream_output);
        if !supports_streaming {
            return Err(Error::CapabilityUnavailable(format!(
                "agent client {} does not support live output",
                session.client_type
            )));
        }

        let expected_runtime = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .runtime_instance_id(&identity.session_id)
            .await?;
        if expected_runtime.as_deref() != Some(producer.runtime_instance_id.as_str()) {
            return Err(Error::StateConflict(format!(
                "runtime_instance_id does not match session {} runtime binding",
                identity.session_id
            )));
        }

        let turn = SqliteTurnRepository::new(self.pool.clone())
            .get_projection(&identity.turn_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("turn {} not found", identity.turn_id)))?;
        if turn.session_id != identity.session_id {
            return Err(Error::StateConflict(format!(
                "turn {} belongs to session {}, not {}",
                identity.turn_id, turn.session_id, identity.session_id
            )));
        }
        let turn_state = turn.state.parse::<TurnState>()?;
        if require_active_turn && turn_state != TurnState::Running {
            return Err(Error::StateConflict(format!(
                "turn {} is {turn_state}, not running",
                identity.turn_id
            )));
        }
        Ok(())
    }
}
