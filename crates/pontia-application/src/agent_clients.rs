use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::agent_clients::{
    AgentClientRow, SqliteAgentClientRepository,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentClientDetails {
    pub id: String,
    pub client_type: String,
    pub client_key: String,
    pub name: String,
    pub short_id: String,
    pub enabled: bool,
    pub has_record: bool,
    pub config_overrides: Value,
    pub effective_config: Value,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
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

    pub async fn list(&self) -> Result<Vec<AgentClientDetails>> {
        let mut identities = BTreeSet::new();
        for entry in self.clients.registrations() {
            if let Some(configuration) = &entry.configuration {
                for key in configuration.keys() {
                    identities.insert((entry.spec.client_type.to_string(), key));
                }
            }
        }
        for row in self.repository.list().await? {
            identities.insert((row.client_type, row.client_key));
        }
        let mut result = Vec::new();
        for (client, key) in identities {
            result.push(self.get(&client, &key).await?);
        }
        Ok(result)
    }

    pub async fn get(&self, client: &str, key: &str) -> Result<AgentClientDetails> {
        let configuration = self
            .configuration(client)
            .map_err(|_| Error::NotFound(format!("Client not found: {client}")))?;
        let record = self.repository.get(client, key).await?;
        if record.is_none() && !configuration.keys().iter().any(|known| known == key) {
            return Err(Error::NotFound("Client entry not found".into()));
        }
        let overrides = record
            .as_ref()
            .map(AgentClientRow::overrides)
            .transpose()?
            .unwrap_or_else(|| json!({}));
        let effective = configuration.merged(key, &overrides);
        let (name, short_id) = configuration.presentation(key, &effective);
        Ok(AgentClientDetails {
            id: URL_SAFE_NO_PAD.encode(serde_json::to_vec(&(client, key))?),
            client_type: client.into(),
            client_key: key.into(),
            name,
            short_id,
            enabled: record.as_ref().is_none_or(|row| row.enabled),
            has_record: record.is_some(),
            config_overrides: overrides,
            effective_config: effective,
            created_at: record.as_ref().map(|row| row.created_at.clone()),
            updated_at: record.map(|row| row.updated_at),
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
            self.configuration(client)?
                .resolve(key, overrides)
                .map_err(|error| Error::Domain(error.to_string()))?;
        } else {
            // Loading persistence errors must not turn into validation errors.
            let current = self.repository.get(client, key).await?;
            effective_configuration(self.configuration(client)?, key, current)
                .map_err(|error| Error::Domain(error.to_string()))?;
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
