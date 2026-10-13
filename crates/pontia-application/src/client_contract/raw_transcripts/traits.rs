use pontia_core::Result;

use serde_json::Value;

use super::{
    AgentBindingResolveRequest, CapturedTimelineBoundary, ManagedToolUse, ResolvedAgentBinding,
    TimelineBoundaryCaptureRequest, TurnTimelineItem, TurnTimelineReadError,
    TurnTimelineReadRequest,
};

pub trait AgentBindingResolver {
    fn client_type(&self) -> &'static str;
    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding>;
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
    /// Return messages and successful compaction checkpoints in native occurrence order.
    /// Session-level checkpoints keep turn_id=None. For pagination, checkpoints in a gap
    /// belong to the page selecting the next Turn; trailing checkpoints belong to the
    /// last Turn's page. With no Turns, return all native checkpoints.
    fn read_session_timeline(
        &self,
        _request: super::SessionTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
        Err(pontia_core::Error::CapabilityUnavailable(
            "Session compaction history is unsupported".into(),
        )
        .into())
    }
    fn read_turn_ranges(
        &self,
        request: TurnTimelineReadRequest,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>;
}

pub trait ToolUseParser {
    fn parse_tool_use(&self, tool_name: &str, arguments: &Value) -> Option<ManagedToolUse>;
}
