use pontia_agent_control::client_contract::raw_transcripts::{TimelineItem, TurnTimelineItem};
use pontia_core::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(super) fn timeline(
    turns: &[Value],
    associations: &HashMap<String, String>,
    selected: &[String],
) -> Result<Vec<TurnTimelineItem>> {
    let mut output = Vec::new();
    for (index, turn) in turns.iter().enumerate() {
        let native = turn["id"]
            .as_str()
            .ok_or_else(|| Error::Domain("Codex Turn has no identity".into()))?;
        let owner = associations.get(native);
        let page = owner
            .or_else(|| {
                turns[index + 1..]
                    .iter()
                    .find_map(|turn| turn["id"].as_str().and_then(|id| associations.get(id)))
            })
            .or_else(|| {
                turns[..index]
                    .iter()
                    .rev()
                    .find_map(|turn| turn["id"].as_str().and_then(|id| associations.get(id)))
            });
        if page.is_some_and(|id| !selected.contains(id)) {
            continue;
        }
        let items = turn["items"]
            .as_array()
            .ok_or_else(|| Error::Domain("Codex full Turn has no items".into()))?;
        for item in items {
            let raw_kind = item["type"]
                .as_str()
                .ok_or_else(|| Error::Domain("Codex history item has no type".into()))?;
            if owner.is_none() && raw_kind != "contextCompaction" {
                continue;
            }
            let id = item["id"]
                .as_str()
                .ok_or_else(|| Error::Domain("Codex history item has no identity".into()))?;
            let (kind, role, content) = match raw_kind {
                "userMessage" => ("user", "user", parts(&item["content"])),
                "agentMessage" => (
                    "assistant",
                    "assistant",
                    item["text"].as_str().unwrap_or_default().into(),
                ),
                "reasoning" => ("thinking", "assistant", parts(&item["summary"])),
                "contextCompaction" => ("context_compaction", "system", String::new()),
                _ => ("tool_result", "tool", item.to_string()),
            };
            output.push(TurnTimelineItem {
                turn_id: owner.cloned(),
                item: TimelineItem {
                    item_id: format!("codex:{native}:{id}"),
                    kind: kind.into(),
                    raw_kind: Some(raw_kind.into()),
                    role: role.into(),
                    title: (role == "tool").then(|| raw_kind.into()),
                    status: item["status"].as_str().map(str::to_owned),
                    occurred_at: item["completedAtMs"]
                        .as_i64()
                        .or_else(|| item["startedAtMs"].as_i64())
                        .map(|millis| {
                            OffsetDateTime::from_unix_timestamp_nanos(
                                i128::from(millis) * 1_000_000,
                            )
                            .map_err(|error| {
                                Error::Domain(format!("Invalid Codex item timestamp: {error}"))
                            })?
                            .format(&Rfc3339)
                            .map_err(|error| {
                                Error::Domain(format!("Invalid Codex item timestamp: {error}"))
                            })
                        })
                        .transpose()?,
                    content_preview: content,
                    call_id: (role == "tool").then(|| id.into()),
                    managed_tool_use: None,
                },
            });
        }
    }
    Ok(output)
}
fn parts(value: &Value) -> String {
    value
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .map(|part| {
                    part.as_str()
                        .or_else(|| part["text"].as_str())
                        .map(str::to_owned)
                        .unwrap_or_else(|| part.to_string())
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
