use serde_json::Value;

use pontia_core::{Error, Result};

use super::refs::decode_pi_content_ref;
use crate::raw_transcripts::{
    TimelineItemDetailPage, TimelineItemDetailReadRequest, TimelineItemDetailReader,
    read_range_from_source, source_len,
};

#[derive(Debug, Clone, Default)]
pub struct PiTimelineItemDetailReader;

impl PiTimelineItemDetailReader {
    pub fn new() -> Self {
        Self
    }
}

impl TimelineItemDetailReader for PiTimelineItemDetailReader {
    fn client_type(&self) -> &'static str {
        "pi"
    }

    fn format(&self) -> &'static str {
        "pi-jsonl"
    }

    fn read_timeline_item_detail(
        &self,
        request: TimelineItemDetailReadRequest,
    ) -> Result<TimelineItemDetailPage> {
        let detail_ref = decode_pi_content_ref(&request.content_ref, &request.source.id)?;
        let source_len = source_len(&request.source)?;
        if detail_ref.start > detail_ref.end || detail_ref.end > source_len {
            return Err(Error::Domain(
                "content_ref_invalid: byte range outside source".to_string(),
            ));
        }
        let bytes = read_range_from_source(&request.source, detail_ref.start, detail_ref.end)?;
        let line = std::str::from_utf8(&bytes)
            .map_err(|err| {
                Error::Domain(format!("content_ref_invalid: source is not utf-8: {err}"))
            })?
            .trim_end_matches(['\r', '\n']);
        let entry: Value = serde_json::from_str(line)?;
        let text = match detail_ref.kind.as_str() {
            "assistant" | "thinking" | "tool_call" => entry
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| content.get(detail_ref.block_index))
                .cloned()
                .unwrap_or(Value::Null),
            _ => entry.clone(),
        };
        let text = serde_json::to_string_pretty(&text)?;
        Ok(TimelineItemDetailPage {
            binding_id: request.source.id,
            content_ref: request.content_ref,
            content_type: "application/json".to_string(),
            size_bytes: text.len(),
            text,
        })
    }
}
