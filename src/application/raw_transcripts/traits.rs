use crate::error::Result;

use super::{
    AgentBindingResolveRequest, ResolvedAgentBinding, TimelineItemDetailPage,
    TimelineItemDetailRequest, TimelinePage, TimelinePageRequest, TimelineUpdatesPage,
    TimelineUpdatesRequest,
};

pub trait AgentBindingResolver {
    fn client_type(&self) -> &'static str;
    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding>;
}

pub trait RawTranscriptParser {
    fn client_type(&self) -> &'static str;
    fn format(&self) -> &'static str;
    fn timeline_page(&self, request: TimelinePageRequest) -> Result<TimelinePage>;
    fn timeline_updates(&self, request: TimelineUpdatesRequest) -> Result<TimelineUpdatesPage>;
    fn timeline_item_detail(
        &self,
        request: TimelineItemDetailRequest,
    ) -> Result<TimelineItemDetailPage>;
}
