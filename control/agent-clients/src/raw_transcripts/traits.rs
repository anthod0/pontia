use pontia_core::Result;

use serde_json::Value;

use super::{
    AgentBindingResolveRequest, ManagedToolUse, ResolvedAgentBinding, TimelineItemDetailPage,
    TimelineItemDetailRequest, TimelinePage, TimelinePageRequest,
};

pub trait AgentBindingResolver {
    fn client_type(&self) -> &'static str;
    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding>;
}

pub trait RawTranscriptParser {
    fn client_type(&self) -> &'static str;
    fn format(&self) -> &'static str;
    fn timeline_page(&self, request: TimelinePageRequest) -> Result<TimelinePage>;
    fn timeline_item_detail(
        &self,
        request: TimelineItemDetailRequest,
    ) -> Result<TimelineItemDetailPage>;
}

pub trait ToolUseParser {
    fn parse_tool_use(&self, tool_name: &str, arguments: &Value) -> Option<ManagedToolUse>;
}
