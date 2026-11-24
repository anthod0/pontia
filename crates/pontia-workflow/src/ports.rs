use std::future::Future;

use pontia_agent_control::{
    AgentControl, AgentEventBroker, CreateSessionRequest, SessionCommandService,
};
use pontia_core::domain::DomainEvent;

use crate::{Error, Result};

pub struct ProfileSessionConfig {
    pub mode: pontia_core::SessionMode,
    pub client_type: String,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub metadata: serde_json::Value,
    pub system_instructions: Option<String>,
}

pub trait SessionCreator {
    fn resolve_profile(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<ProfileSessionConfig>> + Send;

    fn default_client_type(&self) -> Result<String>;
    fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> impl Future<Output = Result<String>> + Send;
}

impl SessionCreator for AgentControl {
    async fn resolve_profile(&self, id: &str) -> Result<ProfileSessionConfig> {
        let profiles = self.agent_profiles();
        let profile = profiles.resolve_launch(id).await?;
        let (client_type, metadata) =
            if let (Some(client), Some(key)) = (&profile.client_type, &profile.client_key) {
                (
                    client.clone(),
                    self.clients()
                        .creation_metadata(&self.db(), client, key)
                        .await?,
                )
            } else {
                (self.default_client_type()?, serde_json::json!({}))
            };
        Ok(ProfileSessionConfig {
            mode: profile.mode,
            client_type,
            model: profile.model,
            reasoning_effort: profile.reasoning_effort,
            metadata,
            system_instructions: profile.system_prompt,
        })
    }

    fn default_client_type(&self) -> Result<String> {
        Ok(self.session_commands().default_client_type()?.to_string())
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
