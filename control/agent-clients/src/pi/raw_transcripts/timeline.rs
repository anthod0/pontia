use pontia_core::{Error, Result};

use crate::raw_transcripts::{
    CapturedTimelineBoundary, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
    TimelineBoundaryCapturer, source_len,
};

const CURSOR_PREFIX: &str = "pi-jsonl-v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineBoundaryRelation {
    After,
}

impl TimelineBoundaryRelation {
    fn as_str(self) -> &'static str {
        match self {
            Self::After => "after",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiJsonlV2Cursor {
    pub binding_id: String,
    pub byte_offset: usize,
    pub native_entry_anchor: Option<String>,
    pub relation: TimelineBoundaryRelation,
}

impl PiJsonlV2Cursor {
    pub fn encode(&self) -> String {
        format!(
            "{CURSOR_PREFIX}:{}:{}:{}:{}",
            self.binding_id,
            self.byte_offset,
            self.relation.as_str(),
            self.native_entry_anchor.as_deref().unwrap_or_default()
        )
    }

    pub fn decode(cursor: &str, expected_binding_id: &str) -> Result<Self> {
        let parts: Vec<_> = cursor.splitn(5, ':').collect();
        if parts.len() != 5 || parts[0] != CURSOR_PREFIX {
            return Err(Error::Domain(
                "cursor_invalid: pi cursor format mismatch".to_string(),
            ));
        }
        if parts[1] != expected_binding_id {
            return Err(Error::Domain(
                "cursor_invalid: pi cursor scope mismatch".to_string(),
            ));
        }
        let byte_offset = parts[2]
            .parse()
            .map_err(|_| Error::Domain("cursor_invalid: invalid byte offset".to_string()))?;
        let relation = match parts[3] {
            "after" => TimelineBoundaryRelation::After,
            _ => {
                return Err(Error::Domain(
                    "cursor_invalid: invalid boundary relation".to_string(),
                ));
            }
        };
        let native_entry_anchor = (!parts[4].is_empty()).then(|| parts[4].to_string());
        Ok(Self {
            binding_id: parts[1].to_string(),
            byte_offset,
            native_entry_anchor,
            relation,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PiTimelineAdapter;

impl PiTimelineAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl TimelineBoundaryCapturer for PiTimelineAdapter {
    fn client_type(&self) -> &'static str {
        "pi"
    }

    fn capture_boundary(
        &self,
        request: TimelineBoundaryCaptureRequest,
    ) -> Result<CapturedTimelineBoundary> {
        if request.source.client_type != self.client_type() || request.source.format != "pi-jsonl" {
            return Err(Error::CapabilityUnavailable(format!(
                "unsupported source {}/{} for pi timeline adapter",
                request.source.client_type, request.source.format
            )));
        }
        let missing_anchor_allowed = request.kind == TimelineBoundaryCaptureKind::Head
            && request.allow_missing_native_entry_anchor;
        if request.native_entry_anchor.is_none() && !missing_anchor_allowed {
            return Err(Error::Domain(
                "cursor_invalid: native entry anchor is required".to_string(),
            ));
        }
        let cursor = PiJsonlV2Cursor {
            binding_id: request.source.id.clone(),
            byte_offset: source_len(&request.source)?,
            native_entry_anchor: request.native_entry_anchor,
            relation: TimelineBoundaryRelation::After,
        }
        .encode();
        Ok(CapturedTimelineBoundary {
            kind: request.kind,
            cursor,
        })
    }
}
