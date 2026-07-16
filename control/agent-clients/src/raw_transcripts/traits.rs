use pontia_core::Result;

use serde_json::Value;

use super::{
    AgentBindingResolveRequest, CapturedTimelineBoundary, ManagedToolUse, ResolvedAgentBinding,
    TimelineBoundaryCaptureRequest, TimelineItemDetailPage, TimelineItemDetailReadRequest,
    TurnTimelineItem, TurnTimelineReadError, TurnTimelineReadRequest,
};

pub trait AgentBindingResolver {
    fn client_type(&self) -> &'static str;
    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding>;
}

pub trait TimelineItemDetailReader {
    fn client_type(&self) -> &'static str;
    fn format(&self) -> &'static str;
    fn read_timeline_item_detail(
        &self,
        request: TimelineItemDetailReadRequest,
    ) -> Result<TimelineItemDetailPage>;
}

pub trait TimelineBoundaryCapturer {
    fn client_type(&self) -> &'static str;
    fn capture_boundary(
        &self,
        request: TimelineBoundaryCaptureRequest,
    ) -> Result<CapturedTimelineBoundary>;
    fn capture_source_origin_head(
        &self,
        binding_id: &str,
        native_entry_anchor: Option<String>,
    ) -> Result<CapturedTimelineBoundary>;
}

pub trait TurnTimelineReader {
    fn client_type(&self) -> &'static str;
    fn read_turn_ranges(
        &self,
        request: TurnTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>;
}

pub trait ToolUseParser {
    fn parse_tool_use(&self, tool_name: &str, arguments: &Value) -> Option<ManagedToolUse>;
}
