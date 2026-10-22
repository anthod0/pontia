use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveToolPhase {
    Running,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveToolOutcome {
    Succeeded,
    Failed,
    Rejected,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveToolStatus {
    pub phase: LiveToolPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<LiveToolOutcome>,
}

impl LiveToolStatus {
    /// Late starts and incomplete replays cannot erase confirmed execution facts.
    pub fn merge(&mut self, incoming: Self) {
        if self.phase == LiveToolPhase::Ended
            && (incoming.phase == LiveToolPhase::Running || self.outcome.is_some())
        {
            return;
        }
        *self = incoming;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveToolOutput {
    pub text: String,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveToolResult {
    #[serde(
        default,
        deserialize_with = "present_json",
        skip_serializing_if = "Option::is_none"
    )]
    pub content: Option<Value>,
    #[serde(
        default,
        deserialize_with = "present_json",
        skip_serializing_if = "Option::is_none"
    )]
    pub error: Option<Value>,
    #[serde(default)]
    pub truncated: bool,
}

impl LiveToolResult {
    pub(super) fn content_bytes(&self) -> usize {
        [&self.content, &self.error]
            .into_iter()
            .flatten()
            .map(|value| match value {
                Value::String(text) => text.len(),
                other => serde_json::to_vec(other).map_or(0, |v| v.len()),
            })
            .sum()
    }
}

impl super::LiveOutputUpdate {
    /// Bound producer-owned tool content to the same budget enforced by the store.
    /// Input calls are never truncated: an incomplete invocation is not a real call.
    pub fn bound_tool_content(self, items: &[super::LiveOutputItem]) -> Self {
        use super::{LiveOutputItem, LiveOutputUpdate};
        let mut used = 0;
        for item in items {
            if let LiveOutputItem::ToolCall {
                call_id,
                arguments,
                managed_tool_use,
                output,
                result,
                ..
            } = item
            {
                used += serde_json::to_vec(&(arguments, managed_tool_use)).map_or(0, |v| v.len());
                let replaces_output = matches!(&self, LiveOutputUpdate::ToolOutputUpdated { call_id: id, .. } if id == call_id);
                let replaces_result = matches!(&self, LiveOutputUpdate::ToolResult { call_id: id, .. } if id == call_id);
                if !replaces_output {
                    used += output.as_ref().map_or(0, |o| o.text.len());
                }
                if !replaces_result {
                    used += result.as_deref().map_or(0, LiveToolResult::content_bytes);
                }
            }
        }
        let budget = super::validation::MAX_TOTAL_TOOL_BYTES.saturating_sub(used);
        match self {
            LiveOutputUpdate::ToolOutputDelta {
                call_id,
                mut delta,
                mut truncated,
            } => {
                truncated |= truncate_text(&mut delta, budget);
                LiveOutputUpdate::ToolOutputDelta {
                    call_id,
                    delta,
                    truncated,
                }
            }
            LiveOutputUpdate::ToolOutputUpdated {
                call_id,
                mut text,
                mut truncated,
            } => {
                truncated |= truncate_text(&mut text, budget);
                LiveOutputUpdate::ToolOutputUpdated {
                    call_id,
                    text,
                    truncated,
                }
            }
            LiveOutputUpdate::ToolResult {
                call_id,
                mut result,
            } => {
                if result.content_bytes() > budget {
                    result.truncated = true;
                    // Preserve which fields were present. A marked preview is not a fabricated result.
                    let fields =
                        usize::from(result.content.is_some()) + usize::from(result.error.is_some());
                    let allowance = budget / fields.max(1);
                    for value in [&mut result.content, &mut result.error]
                        .into_iter()
                        .flatten()
                    {
                        let mut text = match &*value {
                            Value::String(text) => text.clone(),
                            other => other.to_string(),
                        };
                        truncate_text(&mut text, allowance);
                        *value = Value::String(text);
                    }
                }
                LiveOutputUpdate::ToolResult { call_id, result }
            }
            other => other,
        }
    }
}

fn truncate_text(text: &mut String, budget: usize) -> bool {
    if text.len() <= budget {
        return false;
    }
    let mut end = budget;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    true
}

// JSON null is an actual native value, not an absent result.
fn present_json<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
