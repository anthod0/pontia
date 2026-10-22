use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::agent_clients::{
    AgentClientRow, SqliteAgentClientRepository,
};
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::{client_contract::ClientConfiguration, clients::ClientRegistry};

fn effective_configuration(
    configuration: &dyn ClientConfiguration,
    key: &str,
    record: Option<AgentClientRow>,
) -> Result<Value> {
    let overrides = record
        .map(|row| row.overrides())
        .transpose()?
        .unwrap_or_else(|| json!({}));
    configuration.resolve(key, &overrides)
}

/// Management settings are not evidence of client installation, connection, or execution.
#[derive(Clone)]
pub struct AgentClientService {
    repository: SqliteAgentClientRepository,
    clients: ClientRegistry,
}

impl AgentClientService {
    pub fn new(pool: SqlitePool, clients: ClientRegistry) -> Self {
        Self {
            repository: SqliteAgentClientRepository::new(pool),
            clients,
        }
    }

    fn configuration(&self, client: &str) -> Result<&dyn ClientConfiguration> {
        self.clients
            .get(client)
            .and_then(|entry| entry.configuration.as_deref())
            .ok_or_else(|| {
                Error::CapabilityUnavailable(format!("Client configuration unavailable: {client}"))
            })
    }

    pub async fn resolve(&self, client: &str, key: &str) -> Result<Value> {
        effective_configuration(
            self.configuration(client)?,
            key,
            self.repository.get(client, key).await?,
        )
    }

    /// Validate the final configuration, not the partial database layer. Disabled entries
    /// must still be valid. Each write replaces the supplied database object atomically.
    pub async fn save(
        &self,
        client: &str,
        key: &str,
        enabled: Option<bool>,
        overrides: Option<Value>,
    ) -> Result<()> {
        if enabled.is_none() && overrides.is_none() {
            return Err(Error::Domain("No client setting supplied".into()));
        }
        if let Some(overrides) = &overrides {
            if !overrides.is_object() {
                return Err(Error::Domain("Client overrides must be an object".into()));
            }
            self.configuration(client)?.resolve(key, overrides)?;
        } else {
            self.resolve(client, key).await?;
        }
        let encoded = overrides
            .map(|value| serde_json::to_string(&value))
            .transpose()?;
        self.repository
            .save(client, key, enabled, encoded.as_deref())
            .await
    }

    pub async fn delete(&self, client: &str, key: &str) -> Result<()> {
        self.repository.delete(client, key).await
    }

    /// Read once before creating any durable Session or launching a runtime. Existing
    /// Session control and recovery deliberately do not use this admission check.
    pub async fn for_creation(&self, client: &str, metadata: &Value) -> Result<Option<Value>> {
        let entry = self
            .clients
            .get(client)
            .ok_or_else(|| Error::CapabilityUnavailable(format!("Unknown client: {client}")))?;
        let key = match &entry.configuration {
            Some(configuration) => configuration.key(metadata)?,
            None => "default".into(),
        };
        let record = self.repository.get(client, &key).await?;
        if record.as_ref().is_some_and(|row| !row.enabled) {
            return Err(Error::CapabilityUnavailable(format!(
                "Client {client}/{key} is disabled"
            )));
        }
        entry
            .configuration
            .as_ref()
            .map(|configuration| effective_configuration(configuration.as_ref(), &key, record))
            .transpose()
    }
}
