use sqlx::SqlitePool;

use pontia_agent_clients as agent_clients;
use pontia_core::error::Error;

use crate::{AgentBindingService, ExternalQueryService, agent_bindings::AgentBinding};

use super::{AgentBindingResolveRequest, TimelineItemDetailPage, TimelineItemDetailReadRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineItemDetailErrorCode {
    CapabilityUnavailable,
    NotReady,
    SourceUnavailable,
    ContentRefInvalid,
}

impl TimelineItemDetailErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CapabilityUnavailable => "capability_unavailable",
            Self::NotReady => "not_ready",
            Self::SourceUnavailable => "source_unavailable",
            Self::ContentRefInvalid => "content_ref_invalid",
        }
    }
}

#[derive(Debug)]
pub enum TimelineItemDetailServiceError {
    NotFound(String),
    Detail {
        code: TimelineItemDetailErrorCode,
        message: String,
    },
    Inner(Error),
}

impl From<Error> for TimelineItemDetailServiceError {
    fn from(error: Error) -> Self {
        Self::Inner(error)
    }
}

#[derive(Clone)]
pub struct TimelineItemDetailService {
    pool: SqlitePool,
}

impl TimelineItemDetailService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn read(
        &self,
        session_id: String,
        content_ref: String,
    ) -> std::result::Result<TimelineItemDetailPage, TimelineItemDetailServiceError> {
        let (binding, session_state) = self.binding_and_session_state(&session_id).await?;
        let backend = detail_backend(&binding)?;
        let source = backend
            .resolver
            .resolve(&AgentBindingResolveRequest {
                id: binding.id.clone(),
                session_id: binding.session_id.clone(),
                client_type: binding.client_type.clone(),
                launch_cwd: binding.launch_cwd.clone().into(),
                client_session_key: binding.client_session_key.clone(),
            })
            .map_err(|error| detail_error_from_error(error, binding.discovered, &session_state))?;
        let detail = backend
            .reader
            .read_timeline_item_detail(TimelineItemDetailReadRequest {
                session_id,
                source,
                content_ref,
            })
            .map_err(|error| detail_error_from_error(error, binding.discovered, &session_state))?;
        if !binding.discovered {
            AgentBindingService::new(self.pool.clone())
                .mark_discovered(&binding.id)
                .await?;
        }
        Ok(detail)
    }

    async fn binding_and_session_state(
        &self,
        session_id: &str,
    ) -> std::result::Result<(AgentBinding, String), TimelineItemDetailServiceError> {
        let query_service = ExternalQueryService::new(self.pool.clone());
        let session = query_service
            .get_session(session_id)
            .await?
            .ok_or_else(|| {
                TimelineItemDetailServiceError::NotFound(format!("session {session_id} not found"))
            })?;

        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
            .ok_or_else(|| {
                detail_error(
                    TimelineItemDetailErrorCode::NotReady,
                    format!("session {session_id} has no agent binding"),
                )
            })?;

        Ok((binding, session.state))
    }
}

fn detail_backend(
    binding: &AgentBinding,
) -> std::result::Result<agent_clients::TimelineItemDetailBackend, TimelineItemDetailServiceError> {
    agent_clients::timeline_item_detail_backend_for(&binding.client_type).ok_or_else(|| {
        detail_error(
            TimelineItemDetailErrorCode::CapabilityUnavailable,
            format!(
                "{} client does not support timeline item detail",
                binding.client_type
            ),
        )
    })
}

fn detail_error_from_error(
    error: Error,
    discovered: bool,
    session_state: &str,
) -> TimelineItemDetailServiceError {
    let message = error.to_string();
    if message.contains("source_unavailable:") {
        let code = if discovered && matches!(session_state, "exited" | "error") {
            TimelineItemDetailErrorCode::SourceUnavailable
        } else {
            TimelineItemDetailErrorCode::NotReady
        };
        return detail_error(code, message);
    }
    if message.contains("content_ref_invalid:") {
        return detail_error(TimelineItemDetailErrorCode::ContentRefInvalid, message);
    }
    TimelineItemDetailServiceError::Inner(error)
}

fn detail_error(
    code: TimelineItemDetailErrorCode,
    message: impl Into<String>,
) -> TimelineItemDetailServiceError {
    TimelineItemDetailServiceError::Detail {
        code,
        message: message.into(),
    }
}
