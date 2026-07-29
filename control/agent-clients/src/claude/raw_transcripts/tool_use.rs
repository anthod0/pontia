use serde_json::Value;

use crate::raw_transcripts::{ManagedToolUse, ManagedToolUseInput, ToolUseParser};

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ClaudeToolUseParser;

impl ToolUseParser for ClaudeToolUseParser {
    fn parse_tool_use(&self, tool_name: &str, input: &Value) -> Option<ManagedToolUse> {
        let normalized = tool_name.rsplit("__").next().unwrap_or(tool_name);
        let managed_input = match normalized.to_ascii_lowercase().as_str() {
            "read" => {
                let start_line = u64_field(input, "offset");
                let end_line = match (start_line, u64_field(input, "limit")) {
                    (Some(start), Some(limit)) if limit > 0 => Some(start + limit - 1),
                    _ => None,
                };
                ManagedToolUseInput::Read {
                    path: string_field(input, "file_path")?,
                    start_line,
                    end_line,
                }
            }
            "edit" => ManagedToolUseInput::Edit {
                path: string_field(input, "file_path")?,
                edits_count: 1,
            },
            "multiedit" => ManagedToolUseInput::Edit {
                path: string_field(input, "file_path")?,
                edits_count: input
                    .get("edits")
                    .and_then(Value::as_array)
                    .map(|edits| edits.len() as u64)
                    .unwrap_or(0),
            },
            "write" => ManagedToolUseInput::Write {
                path: string_field(input, "file_path")?,
            },
            "bash" => ManagedToolUseInput::Bash {
                command: string_field(input, "command")?,
                timeout: u64_field(input, "timeout"),
            },
            _ => return None,
        };

        Some(ManagedToolUse {
            tool_name: tool_name.to_string(),
            input: managed_input,
        })
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}
