use std::future::Future;

use pontia_agent_control::{
    AgentControl, AgentEventBroker, CreateSessionRequest, SessionCommandService, TurnCommandService,
};
use pontia_core::domain::DomainEvent;

use crate::{Error, Result};

pub struct ProfileSessionConfig {
    pub client_type: String,
    pub model: Option<String>,
    pub metadata: serde_json::Value,
    pub system_instructions: Option<String>,
}

pub trait SessionCreator {
    fn resolve_profile(
        &self,
        id: &str,
        version: Option<&str>,
    ) -> impl Future<Output = Result<ProfileSessionConfig>> + Send {
        let _ = version;
        async move { Err(pontia_core::Error::NotFound(format!("agent profile {id} not found")).into()) }
    }

    fn default_client_type(&self) -> Result<String>;
    fn find_session_by_ownership(
        &self,
        _ownership: &pontia_agent_control::SessionOwnership,
    ) -> impl Future<Output = Result<Option<String>>> + Send {
        async { Ok(None) }
    }

    fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> impl Future<Output = Result<String>> + Send;
}

impl SessionCreator for AgentControl {
    async fn resolve_profile(
        &self,
        id: &str,
        version: Option<&str>,
    ) -> Result<ProfileSessionConfig> {
        let clients = self.clients();
        let profile = pontia_agent_control::AgentProfileService::new(self.db())
            .with_clients(clients.clone())
            .resolve_launch(id, version)
            .await?;
        let (client_type, model, metadata) =
            if let (Some(client_type), Some(client_key), Some(model)) = (
                profile.default_client_type,
                profile.default_client_key,
                profile.default_model,
            ) {
                let configuration = clients
                    .get(&client_type)
                    .and_then(|entry| entry.configuration.as_ref())
                    .ok_or_else(|| {
                        pontia_core::Error::CapabilityUnavailable(format!(
                            "Client configuration unavailable: {client_type}"
                        ))
                    })?;
                let mut metadata = None;
                for option in clients.creation_options(&self.db()).await? {
                    if option.client_type == client_type
                        && configuration.key(&option.metadata)? == client_key
                    {
                        metadata = Some(option.metadata);
                        break;
                    }
                }
                let metadata = metadata.ok_or_else(|| {
                    pontia_core::Error::CapabilityUnavailable(format!(
                        "Client {client_type}/{client_key} is unavailable for Session creation"
                    ))
                })?;
                (client_type, Some(model), metadata)
            } else {
                (self.default_client_type()?, None, serde_json::json!({}))
            };
        if !profile.supported_client_types.contains(&client_type) {
            return Err(pontia_core::Error::Domain(format!(
                "agent profile {id} does not declare {client_type} support"
            ))
            .into());
        }
        Ok(ProfileSessionConfig {
            client_type,
            model,
            metadata,
            system_instructions: profile.system_prompt_template,
        })
    }

    fn default_client_type(&self) -> Result<String> {
        Ok(self.session_commands().default_client_type()?.to_string())
    }
    async fn find_session_by_ownership(
        &self,
        ownership: &pontia_agent_control::SessionOwnership,
    ) -> Result<Option<String>> {
        Ok(self.queries().find_session_by_ownership(ownership).await?)
    }

    async fn create_session(&self, request: CreateSessionRequest) -> Result<String> {
        let outcome = self.session_commands().create_session(request).await?;
        outcome
            .session_id()
            .map(str::to_string)
            .ok_or(Error::MissingCreatedSessionId)
    }
}

pub trait GracefulExitRequester: Sync {
    fn ensure_current_runtime(
        &self,
        session_id: &str,
        runtime_id: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    fn ensure_control_target(
        &self,
        session_id: &str,
        runtime_id: Option<&str>,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let runtime_id = runtime_id.ok_or_else(|| Error::RuntimeControlUnavailable {
                session_id: session_id.to_string(),
                message: "runtime identity is required".into(),
            })?;
            self.ensure_current_runtime(session_id, runtime_id).await
        }
    }

    fn request_graceful_exit(
        &self,
        session_id: &str,
        runtime_id: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    fn request_graceful_session_exit(
        &self,
        session_id: &str,
        runtime_id: Option<&str>,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let runtime_id = runtime_id.ok_or_else(|| Error::RuntimeControlUnavailable {
                session_id: session_id.to_string(),
                message: "runtime identity is required".into(),
            })?;
            self.request_graceful_exit(session_id, runtime_id).await
        }
    }
}

pub trait TurnInterruptionRequester {
    fn request_turn_interruption(
        &self,
        session_id: &str,
        turn_id: &str,
        runtime_id: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

impl TurnInterruptionRequester for TurnCommandService {
    async fn request_turn_interruption(
        &self,
        session_id: &str,
        turn_id: &str,
        runtime_id: &str,
    ) -> Result<()> {
        self.interrupt_turn_for_runtime(session_id, turn_id, runtime_id)
            .await?;
        Ok(())
    }
}

impl GracefulExitRequester for SessionCommandService {
    async fn ensure_current_runtime(&self, session_id: &str, runtime_id: &str) -> Result<()> {
        SessionCommandService::ensure_current_runtime(self, session_id, runtime_id)
            .await
            .map_err(|error| match error {
                pontia_core::Error::CapabilityUnavailable(message)
                | pontia_core::Error::NotFound(message) => Error::RuntimeControlUnavailable {
                    session_id: session_id.to_string(),
                    message,
                },
                error => error.into(),
            })
    }

    async fn ensure_control_target(
        &self,
        session_id: &str,
        runtime_id: Option<&str>,
    ) -> Result<()> {
        SessionCommandService::ensure_control_target(self, session_id, runtime_id)
            .await
            .map_err(|error| match error {
                pontia_core::Error::CapabilityUnavailable(message)
                | pontia_core::Error::NotFound(message) => Error::RuntimeControlUnavailable {
                    session_id: session_id.to_string(),
                    message,
                },
                error => error.into(),
            })
    }

    async fn request_graceful_exit(&self, session_id: &str, runtime_id: &str) -> Result<()> {
        self.request_exit(session_id, Some(runtime_id))
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn request_graceful_session_exit(
        &self,
        session_id: &str,
        runtime_id: Option<&str>,
    ) -> Result<()> {
        self.request_exit(session_id, runtime_id)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

pub trait AgentEventSubscriber {
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DomainEvent>;
}

impl AgentEventSubscriber for AgentEventBroker {
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DomainEvent> {
        AgentEventBroker::subscribe(self)
    }
}
