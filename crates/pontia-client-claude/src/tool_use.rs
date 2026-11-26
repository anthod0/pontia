use pontia_agent_control::client_contract::raw_transcripts::{ManagedToolUse, ManagedToolUseInput};
use serde_json::Value;

pub(crate) fn from_call(tool_name: &str, arguments: &Value) -> Option<ManagedToolUse> {
    let input = match tool_name {
        "Read" => {
            let start_line = u64_field(arguments, "offset");
            let end_line = match (start_line, u64_field(arguments, "limit")) {
                (Some(start), Some(limit)) => start.checked_add(limit)?.checked_sub(1),
                (None, Some(limit)) => Some(limit),
                _ => None,
            };
            ManagedToolUseInput::Read {
                path: string_field(arguments, "file_path")?,
                start_line,
                end_line,
            }
        }
        "Edit" => ManagedToolUseInput::Edit {
            path: string_field(arguments, "file_path")?,
            edits_count: 1,
        },
        "NotebookEdit" => ManagedToolUseInput::Edit {
            path: string_field(arguments, "notebook_path")?,
            edits_count: 1,
        },
        "Write" => ManagedToolUseInput::Write {
            path: string_field(arguments, "file_path")?,
        },
        "Bash" | "PowerShell" => ManagedToolUseInput::Bash {
            command: string_field(arguments, "command")?,
            timeout: u64_field(arguments, "timeout"),
        },
        _ => return None,
    };
    Some(ManagedToolUse {
        tool_name: tool_name.into(),
        input,
    })
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_claude_file_and_command_tools_to_managed_cards() {
        let cases = [
            (
                "Read",
                json!({"file_path":"src/main.rs","offset":4,"limit":3}),
                json!({"tool_name":"Read","input":{"type":"read","path":"src/main.rs","start_line":4,"end_line":6}}),
            ),
            (
                "Edit",
                json!({"file_path":"src/main.rs","old_string":"old","new_string":"new"}),
                json!({"tool_name":"Edit","input":{"type":"edit","path":"src/main.rs","edits_count":1}}),
            ),
            (
                "NotebookEdit",
                json!({"notebook_path":"analysis.ipynb","new_source":"value"}),
                json!({"tool_name":"NotebookEdit","input":{"type":"edit","path":"analysis.ipynb","edits_count":1}}),
            ),
            (
                "Write",
                json!({"file_path":"notes.txt","content":"value"}),
                json!({"tool_name":"Write","input":{"type":"write","path":"notes.txt"}}),
            ),
            (
                "Bash",
                json!({"command":"cargo test","timeout":120000}),
                json!({"tool_name":"Bash","input":{"type":"bash","command":"cargo test","timeout":120000}}),
            ),
            (
                "PowerShell",
                json!({"command":"Get-ChildItem","timeout":120000}),
                json!({"tool_name":"PowerShell","input":{"type":"bash","command":"Get-ChildItem","timeout":120000}}),
            ),
        ];
        for (name, arguments, expected) in cases {
            assert_eq!(
                serde_json::to_value(from_call(name, &arguments)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn leaves_unknown_and_malformed_tools_unmanaged() {
        assert_eq!(from_call("Glob", &json!({"pattern":"**/*.rs"})), None);
        assert_eq!(from_call("Read", &json!({"file_path":7})), None);
        assert_eq!(from_call("Write", &json!({"content":"missing path"})), None);
    }
}
