use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use std::{collections::BTreeSet, io, path::Path};
use uuid::Uuid;

use crate::{
    client_contract::{
        ClientConfiguration, ClientDetection, ClientSetup, DetectionContext, DetectionStatus,
        install::{AuthGuidance, InstallMethod, InstallProvenance, InstallSpec},
    },
    clients::ClientRegistry,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentClientDetails {
    pub id: String,
    pub client_type: String,
    pub client_key: String,
    pub name: String,
    pub short_id: String,
    pub enabled: bool,
    pub detection: ClientDetection,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup: Option<ClientSetup>,
    pub setup_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<AgentClientInstallDetails>,
    pub has_record: bool,
    /// Pontia manages this entry's authentication. Performs no I/O.
    pub auth_supported: bool,
    pub version: Option<String>,
    pub version_checked_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentClientInstallDetails {
    pub supported: bool,
    pub provenance: InstallProvenance,
    pub available_actions: Vec<String>,
    pub methods: Vec<AgentClientInstallMethod>,
    pub auth: AuthGuidance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentClientInstallMethod {
    #[serde(flatten)]
    pub method: InstallMethod,
    pub ready: bool,
}

impl From<InstallSpec> for AgentClientInstallDetails {
    fn from(spec: InstallSpec) -> Self {
        let available_actions = if spec.provenance == InstallProvenance::NotInstalled {
            vec!["install".into()]
        } else {
            Vec::new()
        };
        Self {
            supported: true,
            provenance: spec.provenance,
            available_actions,
            methods: spec
                .methods
                .into_iter()
                .map(|method| AgentClientInstallMethod {
                    ready: method.ready(),
                    method,
                })
                .collect(),
            auth: spec.auth,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AgentClientOperationView {
    #[serde(flatten)]
    pub operation: pontia_runtime::client_operations::InstallState,
    pub log_size: u64,
    pub output_offset: u64,
    pub next_offset: u64,
    pub output: String,
}

impl AgentClientOperationView {
    pub fn finished(&self) -> bool {
        self.operation.status != pontia_runtime::client_operations::Status::Running
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientInstallStartError {
    #[error(transparent)]
    Control(#[from] Error),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    PrerequisiteMissing(String),
    #[error("{0}")]
    InProgress(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentClientModels {
    pub models: Vec<crate::sessions::SessionModel>,
    pub default_model: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct CachedModelCatalog {
    #[serde(flatten)]
    pub catalog: AgentClientModels,
    pub version: Option<String>,
    pub fetched_at: i64,
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

    /// Apply each enabled client's directory trust operation to the selected path.
    pub async fn trust_directory(&self, directory: &Path) -> Result<()> {
        let directory = directory.canonicalize()?;
        if !directory.is_dir() {
            return Err(Error::Domain("Directory trust requires a directory".into()));
        }
        for entry in self
            .repository
            .list()
            .await?
            .into_iter()
            .filter(|row| row.enabled)
        {
            self.configuration(&entry.client_type)?
                .trust_directory(&entry.client_key, &directory)
                .await?;
        }
        Ok(())
    }

    pub fn initialize_private_directory(&self, directory: &std::path::Path) -> Result<()> {
        std::fs::create_dir_all(directory)?;
        for file in self
            .clients
            .registrations()
            .filter_map(|entry| entry.configuration.as_ref())
            .flat_map(|config| config.private_directory_files())
        {
            file.install(directory)?;
        }
        Ok(())
    }

    fn configuration(&self, client: &str) -> Result<&dyn ClientConfiguration> {
        self.clients
            .get(client)
            .and_then(|entry| entry.configuration.as_deref())
            .ok_or_else(|| {
                Error::CapabilityUnavailable(format!("Client configuration unavailable: {client}"))
            })
    }

    pub async fn list(&self, context: &DetectionContext<'_>) -> Result<Vec<AgentClientDetails>> {
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
            result.push(self.get(&client, &key, context).await?);
        }
        Ok(result)
    }

    pub async fn get(
        &self,
        client: &str,
        key: &str,
        context: &DetectionContext<'_>,
    ) -> Result<AgentClientDetails> {
        let configuration = self
            .configuration(client)
            .map_err(|_| Error::NotFound(format!("Client not found: {client}")))?;
        let record = self.repository.get(client, key).await?;
        if record.is_none() && !configuration.keys().iter().any(|known| known == key) {
            return Err(Error::NotFound("Client entry not found".into()));
        }
        let effective = configuration.merged(key);
        let (name, short_id) = configuration.presentation(key, &effective);
        let integration = self.clients.integration(client, key);
        let install = self
            .clients
            .installer(client, key)
            .map(|installer| installer.inspect(context).into());
        let setup = match integration {
            Some(integration) => integration
                .detect_setup(context)
                .await
                .unwrap_or_else(|error| ClientSetup::Unknown(error.to_string())),
            None => ClientSetup::NotRequired,
        };
        Ok(AgentClientDetails {
            id: URL_SAFE_NO_PAD.encode(serde_json::to_vec(&(client, key))?),
            client_type: client.into(),
            client_key: key.into(),
            name,
            short_id,
            setup: Some(setup),
            setup_command: integration
                .map(|integration| format!("pontia setup {}", integration.setup_name())),
            install,
            enabled: record.as_ref().is_some_and(|row| row.enabled),
            has_record: record.is_some(),
            auth_supported: self
                .clients
                .get(client)
                .and_then(|registration| registration.auth.as_ref())
                .is_some_and(|auth| auth.supports_auth(key)),
            detection: configuration.detect(&effective, context),
            version: record.as_ref().and_then(|row| row.version.clone()),
            version_checked_at: record
                .as_ref()
                .and_then(|row| row.version_checked_at.clone()),
            created_at: record.as_ref().map(|row| row.created_at.clone()),
            updated_at: record.map(|row| row.updated_at),
        })
    }

    pub async fn probe_version(
        &self,
        client: &str,
        key: &str,
        context: &DetectionContext<'_>,
        force: bool,
    ) -> Result<()> {
        let configuration = self.configuration(client)?;
        let started = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .expect("current UTC timestamp");
        if self
            .repository
            .begin_version_check(client, key, &started, force)
            .await?
            && let Some(version) = configuration.probe_version(key, context).await?
        {
            self.repository
                .record_version(client, key, &started, &version)
                .await?;
        }
        Ok(())
    }

    pub async fn probe_enabled(self, home: std::path::PathBuf, at_startup: bool) {
        let entries = match self.repository.list().await {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(%error, "Cannot list clients for probing");
                return;
            }
        };
        let vars = std::env::vars().collect();
        let context = DetectionContext {
            vars: &vars,
            pontia_home: &home,
        };
        for entry in entries.into_iter().filter(|entry| entry.enabled) {
            let client = &entry.client_type;
            let key = &entry.client_key;
            if let Err(error) = self.probe_version(client, key, &context, at_startup).await {
                tracing::warn!(%error, %client, %key, "Cannot check client version");
            }
            if at_startup
                && let Ok(config) = self.configuration(client)
                && config.supports_startup_model()
                && config.detect(&config.merged(key), &context).status == DetectionStatus::Detected
                && let Err(error) = self.model_catalog(client, key, &home, true).await
            {
                tracing::warn!(%error, %client, %key, "Cannot preload client models");
            }
        }
    }

    pub async fn resolve(&self, client: &str, key: &str) -> Result<Value> {
        self.configuration(client)?.resolve(key)
    }

    pub async fn list_models(
        &self,
        client: &str,
        key: &str,
        directory: &std::path::Path,
    ) -> Result<AgentClientModels> {
        let configuration = self.configuration(client)?;
        let effective = self
            .for_creation_key(client, key)
            .await?
            .expect("client has configuration");
        configuration.list_models(&effective, directory).await
    }

    pub async fn model_catalog(
        &self,
        client: &str,
        key: &str,
        directory: &Path,
        prefer_cache: bool,
    ) -> Result<CachedModelCatalog> {
        if prefer_cache && let Some(saved) = self.repository.model_catalog(client, key).await? {
            self.for_creation_key(client, key).await?;
            return Ok(serde_json::from_str(&saved)?);
        }
        let version = self
            .repository
            .get(client, key)
            .await?
            .and_then(|row| row.version);
        let fetched_at =
            (time::OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64;
        let entry = CachedModelCatalog {
            catalog: self.list_models(client, key, directory).await?,
            version,
            fetched_at,
        };
        self.repository
            .save_model_catalog(
                client,
                key,
                entry.version.as_deref(),
                &serde_json::to_string(&entry)?,
                fetched_at,
            )
            .await?;
        Ok(entry)
    }

    pub async fn validate_model_selection(
        &self,
        client: &str,
        key: &str,
        model: &str,
        reasoning_effort: Option<&str>,
        directory: &std::path::Path,
    ) -> Result<()> {
        if reasoning_effort.is_some()
            && !self
                .configuration(client)?
                .supports_startup_reasoning_effort()
        {
            return Err(Error::CapabilityUnavailable(format!(
                "Client {client}/{key} cannot set reasoning effort"
            )));
        }
        let catalog = self.list_models(client, key, directory).await?;
        let entry = catalog
            .models
            .iter()
            .find(|entry| entry.id == model)
            .ok_or_else(|| {
                Error::Domain(format!("Model {model} is unavailable for {client}/{key}"))
            })?;
        if let Some(effort) = reasoning_effort
            && !entry
                .reasoning_efforts
                .iter()
                .any(|option| option.id == effort)
        {
            return Err(Error::Domain(format!(
                "Reasoning effort {effort} is unavailable for model {model}"
            )));
        }
        Ok(())
    }

    /// Disabled entries must still have a valid effective configuration.
    pub async fn save(&self, client: &str, key: &str, enabled: bool) -> Result<()> {
        self.configuration(client)?
            .resolve(key)
            .map_err(|error| Error::Domain(error.to_string()))?;
        self.repository.save(client, key, enabled).await
    }

    pub async fn delete(&self, client: &str, key: &str) -> Result<()> {
        self.repository.delete(client, key).await
    }

    pub async fn start_install(
        &self,
        home: std::path::PathBuf,
        client: &str,
        key: &str,
        method_id: Option<&str>,
        auto_setup: bool,
        auto_enable: bool,
    ) -> std::result::Result<AgentClientOperationView, ClientInstallStartError> {
        let vars = std::env::vars().collect();
        let context = DetectionContext {
            vars: &vars,
            pontia_home: &home,
        };
        let details = self.get(client, key, &context).await?;
        let installer = self.clients.installer(client, key).ok_or_else(|| {
            ClientInstallStartError::Unavailable(
                "Installation is unavailable for this client".into(),
            )
        })?;
        let spec = installer.inspect(&context);
        if spec.provenance != InstallProvenance::NotInstalled {
            return Err(ClientInstallStartError::Unavailable(
                "Client is already installed".into(),
            ));
        }
        let method = spec
            .methods
            .iter()
            .find(|method| method_id.map_or(method.recommended, |id| method.id == id))
            .ok_or_else(|| {
                ClientInstallStartError::Unavailable(format!(
                    "No matching installation method: {}",
                    method_id.unwrap_or("recommended")
                ))
            })?;
        if !method.ready() {
            let missing = method
                .prerequisites
                .iter()
                .filter(|prerequisite| prerequisite.required && !prerequisite.satisfied)
                .map(|prerequisite| {
                    prerequisite
                        .missing_hint
                        .as_deref()
                        .unwrap_or(&prerequisite.label)
                })
                .collect::<Vec<_>>();
            return Err(ClientInstallStartError::PrerequisiteMissing(
                if missing.is_empty() {
                    "Installation method is unavailable on this platform".into()
                } else {
                    missing.join("\n")
                },
            ));
        }
        let cli = std::env::current_exe()?.with_file_name("pontia");
        let selector = details.short_id;
        let method_id = method.id.clone();
        let operation_home = home.clone();
        let operation = tokio::task::spawn_blocking(move || {
            pontia_runtime::client_operations::start(
                &operation_home,
                &cli,
                &selector,
                &method_id,
                auto_setup,
                auto_enable,
            )
        })
        .await
        .map_err(|error| io::Error::other(error.to_string()))?
        .map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                ClientInstallStartError::InProgress(error.to_string())
            } else {
                ClientInstallStartError::Io(error)
            }
        })?;
        operation_view(&home, operation.id, Some(0), 64 * 1024)
            .await
            .map_err(ClientInstallStartError::from)
    }

    pub async fn latest_operation(
        home: std::path::PathBuf,
        tail_bytes: u64,
    ) -> Result<Option<AgentClientOperationView>> {
        let id = tokio::task::spawn_blocking({
            let home = home.clone();
            move || pontia_runtime::client_operations::latest(&home)
        })
        .await
        .map_err(|error| Error::Domain(error.to_string()))??
        .map(|operation| operation.id);
        match id {
            Some(id) => Ok(Some(operation_view(&home, id, None, tail_bytes).await?)),
            None => Ok(None),
        }
    }

    pub async fn operation(
        home: std::path::PathBuf,
        id: Uuid,
        offset: Option<u64>,
    ) -> Result<AgentClientOperationView> {
        operation_view(&home, id, offset, 64 * 1024).await
    }

    pub async fn cancel_operation(
        home: std::path::PathBuf,
        id: Uuid,
    ) -> Result<AgentClientOperationView> {
        let operation_home = home.clone();
        tokio::task::spawn_blocking(move || {
            pontia_runtime::client_operations::cancel(&operation_home, id)
        })
        .await
        .map_err(|error| Error::Domain(error.to_string()))?
        .map_err(operation_error)?;
        operation_view(&home, id, None, 64 * 1024).await
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
        self.for_creation_key(client, &key).await
    }

    pub(crate) async fn for_creation_key(&self, client: &str, key: &str) -> Result<Option<Value>> {
        let entry = self
            .clients
            .get(client)
            .ok_or_else(|| Error::CapabilityUnavailable(format!("Unknown client: {client}")))?;
        let record = self.repository.get(client, key).await?;
        if record.as_ref().is_none_or(|row| !row.enabled) {
            return Err(Error::CapabilityUnavailable(format!(
                "Client {client}/{key} is disabled"
            )));
        }
        entry
            .configuration
            .as_ref()
            .map(|configuration| configuration.resolve(key))
            .transpose()
    }
}

async fn operation_view(
    home: &Path,
    id: Uuid,
    offset: Option<u64>,
    tail_bytes: u64,
) -> Result<AgentClientOperationView> {
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let operation = pontia_runtime::client_operations::state(&home, id)?;
        let log_size = pontia_runtime::client_operations::log_size(&home, id)?;
        let output_offset = offset
            .unwrap_or_else(|| log_size.saturating_sub(tail_bytes.min(64 * 1024)))
            .min(log_size);
        let bytes = pontia_runtime::client_operations::read_log(&home, id, output_offset)?;
        let next_offset = output_offset + bytes.len() as u64;
        Ok(AgentClientOperationView {
            operation,
            log_size,
            output_offset,
            next_offset,
            output: String::from_utf8_lossy(&bytes).into_owned(),
        })
    })
    .await
    .map_err(|error| Error::Domain(error.to_string()))?
    .map_err(operation_error)
}

fn operation_error(error: io::Error) -> Error {
    match error.kind() {
        io::ErrorKind::NotFound => Error::NotFound("Client operation not found".into()),
        io::ErrorKind::Other => Error::StateConflict(error.to_string()),
        _ => Error::Io(error),
    }
}

impl AgentClientService {
    pub async fn setup(
        &self,
        home: std::path::PathBuf,
        client: &str,
        key: &str,
    ) -> Result<pontia_runtime::client_setup::SetupOperation> {
        self.get(
            client,
            key,
            &DetectionContext {
                vars: &std::env::vars().collect(),
                pontia_home: &home,
            },
        )
        .await?;
        let integration = self.clients.integration(client, key).ok_or_else(|| {
            Error::CapabilityUnavailable("Setup is unavailable for this client".into())
        })?;
        let client = integration.client_type().to_string();
        tokio::task::spawn_blocking(move || {
            let cli = std::env::current_exe()?.with_file_name("pontia");
            pontia_runtime::client_setup::start(
                &home,
                &cli,
                &client,
                &uuid::Uuid::now_v7().to_string(),
            )
            .map_err(Error::Domain)
        })
        .await
        .map_err(|error| Error::Domain(error.to_string()))?
    }

    pub async fn setup_status(
        home: std::path::PathBuf,
    ) -> Result<Option<pontia_runtime::client_setup::SetupOperation>> {
        tokio::task::spawn_blocking(move || {
            pontia_runtime::client_setup::latest(&home).map_err(Error::Domain)
        })
        .await
        .map_err(|error| Error::Domain(error.to_string()))?
    }
}
