use std::future::Future;

use pontia_application::{
    AgentEventBroker, CreateSessionRequest, PiGracefulExitService, RuntimeControlService,
    SessionCommandService,
};
use pontia_core::domain::DomainEvent;

use crate::{Error, Result};

pub trait SessionCreator {
    fn find_session_by_creation_token(
        &self,
        _metadata_key: &str,
        _token: &str,
    ) -> impl Future<Output = Result<Option<String>>> + Send {
        async { Ok(None) }
    }

    fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> impl Future<Output = Result<String>> + Send;
}

impl SessionCreator for SessionCommandService {
    async fn find_session_by_creation_token(
        &self,
        metadata_key: &str,
        token: &str,
    ) -> Result<Option<String>> {
        Ok(
            SessionCommandService::find_session_by_creation_token(self, metadata_key, token)
                .await?,
        )
    }

    async fn create_session(&self, request: CreateSessionRequest) -> Result<String> {
        let outcome = SessionCommandService::create_session(self, request).await?;
        outcome
            .session_id()
            .map(str::to_string)
            .ok_or(Error::MissingCreatedSessionId)
    }
}

pub trait GracefulExitRequester {
    fn ensure_current_runtime(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    fn request_graceful_exit(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

pub trait TurnInterruptionRequester {
    fn request_turn_interruption(
        &self,
        session_id: &str,
        turn_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

impl TurnInterruptionRequester for RuntimeControlService {
    async fn request_turn_interruption(
        &self,
        session_id: &str,
        turn_id: &str,
        runtime_instance_id: &str,
    ) -> Result<()> {
        self.interrupt_turn_for_runtime(session_id, turn_id, runtime_instance_id)
            .await?;
        Ok(())
    }
}

impl GracefulExitRequester for PiGracefulExitService {
    async fn ensure_current_runtime(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<()> {
        PiGracefulExitService::ensure_current_runtime(self, session_id, runtime_instance_id)
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

    async fn request_graceful_exit(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<()> {
        self.request_exit(session_id, runtime_instance_id)
            .await
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
