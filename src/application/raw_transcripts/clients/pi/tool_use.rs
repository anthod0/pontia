use serde_json::Value;

use super::super::super::{ManagedToolUse, ManagedToolUseInput, ToolUseParser};

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct PiToolUseParser;

impl ToolUseParser for PiToolUseParser {
    fn parse_tool_use(&self, tool_name: &str, arguments: &Value) -> Option<ManagedToolUse> {
        let input = match tool_name {
            "read" => ManagedToolUseInput::Read {
                path: string_field(arguments, "path")?,
                start_line: u64_field(arguments, "start_line"),
                end_line: u64_field(arguments, "end_line"),
            },
            "edit" => ManagedToolUseInput::Edit {
                path: string_field(arguments, "path")?,
                edits_count: arguments
                    .get("edits")
                    .and_then(Value::as_array)
                    .map(|edits| edits.len() as u64)
                    .unwrap_or(0),
            },
            "write" => ManagedToolUseInput::Write {
                path: string_field(arguments, "path")?,
            },
            "bash" => ManagedToolUseInput::Bash {
                command: string_field(arguments, "command")?,
                timeout: u64_field(arguments, "timeout"),
            },
            _ => return None,
        };

        Some(ManagedToolUse {
            tool_name: tool_name.to_string(),
            input,
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
