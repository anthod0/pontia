use std::path::PathBuf;

use sqlx::SqlitePool;

use pontia_agent_clients as agent_clients;
use pontia_core::error::{Error, Result};

use crate::{AgentBindingService, ExternalQueryService, agent_bindings::AgentBinding};

use super::{
    AgentBindingResolveRequest, AgentBindingResolver, RawTranscriptParser, TimelineItemDetailPage,
    TimelineItemDetailRequest, TimelinePage, TimelinePageRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawTranscriptTimelineErrorCode {
    CapabilityUnavailable,
    NotReady,
    SourceUnavailable,
    CursorInvalid,
    ContentRefInvalid,
}

impl RawTranscriptTimelineErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CapabilityUnavailable => "capability_unavailable",
            Self::NotReady => "not_ready",
            Self::SourceUnavailable => "source_unavailable",
            Self::CursorInvalid => "cursor_invalid",
            Self::ContentRefInvalid => "content_ref_invalid",
        }
    }
}

#[derive(Debug)]
pub enum RawTranscriptServiceError {
    NotFound(String),
    Timeline {
        code: RawTranscriptTimelineErrorCode,
        message: String,
    },
    Inner(Error),
}

impl From<Error> for RawTranscriptServiceError {
    fn from(error: Error) -> Self {
        Self::Inner(error)
    }
}

#[derive(Clone)]
pub struct RawTranscriptService {
    pool: SqlitePool,
}

impl RawTranscriptService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn timeline_page(
        &self,
        session_id: String,
        before: Option<String>,
        after: Option<String>,
        limit: Option<usize>,
    ) -> std::result::Result<TimelinePage, RawTranscriptServiceError> {
        let (binding, session_state) = self.binding_and_session_state(&session_id).await?;
        let backend = transcript_backend(&binding)?;
        let source = backend
            .resolver
            .resolve(&AgentBindingResolveRequest {
                id: binding.id.clone(),
                session_id: binding.session_id.clone(),
                client_type: binding.client_type.clone(),
                launch_cwd: binding.launch_cwd.clone().into(),
                client_session_key: binding.client_session_key.clone(),
            })
            .map_err(|error| {
                timeline_error_from_binding_error(error, binding.discovered, &session_state)
            })?;
        let page = backend
            .parser
            .timeline_page(TimelinePageRequest {
                session_id,
                source,
                before,
                after,
                limit,
            })
            .map_err(|error| {
                timeline_error_from_error(error, binding.discovered, &session_state)
            })?;
        if !binding.discovered {
            AgentBindingService::new(self.pool.clone())
                .mark_discovered(&binding.id)
                .await?;
        }
        Ok(page)
    }

    pub async fn timeline_item_detail(
        &self,
        session_id: String,
        content_ref: String,
    ) -> std::result::Result<TimelineItemDetailPage, RawTranscriptServiceError> {
        let (binding, session_state) = self.binding_and_session_state(&session_id).await?;
        let backend = transcript_backend(&binding)?;
        let source = backend
            .resolver
            .resolve(&AgentBindingResolveRequest {
                id: binding.id.clone(),
                session_id: binding.session_id.clone(),
                client_type: binding.client_type.clone(),
                launch_cwd: binding.launch_cwd.clone().into(),
                client_session_key: binding.client_session_key.clone(),
            })
            .map_err(|error| {
                timeline_error_from_binding_error(error, binding.discovered, &session_state)
            })?;
        let detail = backend
            .parser
            .timeline_item_detail(TimelineItemDetailRequest {
                session_id,
                source,
                content_ref,
            })
            .map_err(|error| {
                timeline_error_from_error(error, binding.discovered, &session_state)
            })?;
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
    ) -> std::result::Result<(AgentBinding, String), RawTranscriptServiceError> {
        let query_service = ExternalQueryService::new(self.pool.clone());
        let session = query_service
            .get_session(session_id)
            .await?
            .ok_or_else(|| {
                RawTranscriptServiceError::NotFound(format!("session {session_id} not found"))
            })?;

        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
            .ok_or_else(|| {
                timeline_error(
                    RawTranscriptTimelineErrorCode::NotReady,
                    format!("session {session_id} has no agent binding"),
                )
            })?;

        Ok((binding, session.state))
    }
}

pub async fn resolve_and_parse_timeline_page<R, P>(
    binding: &AgentBinding,
    resolver: &R,
    parser: &P,
    cursor: Option<String>,
    limit: usize,
) -> Result<TimelinePage>
where
    R: AgentBindingResolver,
    P: RawTranscriptParser,
{
    let source = resolver.resolve(&AgentBindingResolveRequest {
        id: binding.id.clone(),
        session_id: binding.session_id.clone(),
        client_type: binding.client_type.clone(),
        launch_cwd: PathBuf::from(&binding.launch_cwd),
        client_session_key: binding.client_session_key.clone(),
    })?;
    parser.timeline_page(TimelinePageRequest {
        session_id: binding.session_id.clone(),
        source,
        before: cursor,
        after: None,
        limit: Some(limit),
    })
}

fn transcript_backend(
    binding: &AgentBinding,
) -> std::result::Result<agent_clients::RawTranscriptBackend, RawTranscriptServiceError> {
    agent_clients::raw_transcript_backend_for(&binding.client_type).ok_or_else(|| {
        timeline_error(
            RawTranscriptTimelineErrorCode::CapabilityUnavailable,
            format!(
                "{} client does not support backend transcript timeline",
                binding.client_type
            ),
        )
    })
}

fn timeline_error_from_binding_error(
    error: Error,
    discovered: bool,
    session_state: &str,
) -> RawTranscriptServiceError {
    let message = error.to_string();
    if message.contains("source_unavailable:") {
        return timeline_error(source_unavailable_code(discovered, session_state), message);
    }
    timeline_error_from_error(error, discovered, session_state)
}

fn timeline_error_from_error(
    error: Error,
    discovered: bool,
    session_state: &str,
) -> RawTranscriptServiceError {
    let message = error.to_string();
    if message.contains("source_unavailable:") {
        return timeline_error(source_unavailable_code(discovered, session_state), message);
    }
    if message.contains("cursor_invalid:") {
        return timeline_error(RawTranscriptTimelineErrorCode::CursorInvalid, message);
    }
    if message.contains("content_ref_invalid:") {
        return timeline_error(RawTranscriptTimelineErrorCode::ContentRefInvalid, message);
    }
    RawTranscriptServiceError::Inner(error)
}

fn source_unavailable_code(
    discovered: bool,
    session_state: &str,
) -> RawTranscriptTimelineErrorCode {
    if discovered && matches!(session_state, "exited" | "error") {
        RawTranscriptTimelineErrorCode::SourceUnavailable
    } else {
        RawTranscriptTimelineErrorCode::NotReady
    }
}

fn timeline_error(
    code: RawTranscriptTimelineErrorCode,
    message: impl Into<String>,
) -> RawTranscriptServiceError {
    RawTranscriptServiceError::Timeline {
        code,
        message: message.into(),
    }
}
