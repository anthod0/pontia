use std::path::PathBuf;

use crate::{application::agent_bindings::AgentBinding, error::Result};

use super::{
    AgentBindingResolveRequest, AgentBindingResolver, RawTranscriptParser, TimelinePage,
    TimelinePageRequest,
};

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
        older_cursor: cursor,
        limit,
    })
}
