use pontia_agent_control::client_contract::raw_transcripts::{
    ManagedFileChange, ManagedFileOperation, ManagedToolUse, ManagedToolUseInput, TimelineItem,
};
use serde_json::{Value, json};

pub(crate) fn from_call(name: &str, arguments: &Value) -> Option<ManagedToolUse> {
    let input = match name {
        "commandExecution" | "shell_command" => ManagedToolUseInput::Bash {
            command: arguments["command"].as_str()?.into(),
            timeout: None,
        },
        "exec_command" => ManagedToolUseInput::Bash {
            command: arguments["cmd"].as_str()?.into(),
            timeout: None,
        },
        "fileChange" => {
            let changes = arguments["changes"]
                .as_array()?
                .iter()
                .map(|change| {
                    Some(ManagedFileChange {
                        path: change["path"].as_str()?.into(),
                        operation: match change["kind"]["type"].as_str()? {
                            "add" => ManagedFileOperation::Add,
                            "update" => ManagedFileOperation::Update,
                            "delete" => ManagedFileOperation::Delete,
                            _ => return None,
                        },
                        move_path: change["kind"]["move_path"].as_str().map(str::to_owned),
                        diff: change["diff"].as_str()?.into(),
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            if changes.is_empty() {
                return None;
            }
            ManagedToolUseInput::FileChanges { changes }
        }
        _ => return None,
    };
    Some(ManagedToolUse {
        tool_name: name.into(),
        input,
    })
}

fn from_native(item: &Value) -> Option<ManagedToolUse> {
    let name = item["type"].as_str()?;
    let mut tool_use = match name {
        "commandExecution" | "fileChange" => from_call(name, item)?,
        "CommandExecution" => {
            let argv = item["command"].as_array()?;
            let command = argv
                .iter()
                .map(|arg| arg.as_str().map(shell_word))
                .collect::<Option<Vec<_>>>()?
                .join(" ");
            from_call("commandExecution", &json!({"command":command}))?
        }
        "FileChange" | "patch_apply_end" => {
            let changes = item["changes"]
                .as_object()?
                .iter()
                .map(|(path, change)| {
                    let diff = match change["type"].as_str()? {
                        "add" | "delete" => change["content"].as_str()?,
                        "update" => change["unified_diff"].as_str()?,
                        _ => return None,
                    };
                    Some(json!({
                        "path":path,
                        "kind":{"type":change["type"],"move_path":change["move_path"]},
                        "diff":diff,
                    }))
                })
                .collect::<Option<Vec<_>>>()?;
            from_call("fileChange", &json!({"changes":changes}))?
        }
        _ => return None,
    };
    tool_use.tool_name = name.into();
    Some(tool_use)
}

fn shell_word(word: &str) -> String {
    if !word.is_empty()
        && word
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&byte))
    {
        word.into()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

pub(crate) fn native_timeline(item: &Value, mut entry: TimelineItem) -> Vec<TimelineItem> {
    let Some(tool_use) = from_native(item) else {
        return vec![entry];
    };
    entry.kind = "tool_call".into();
    entry.role = "assistant".into();
    entry.title = Some(tool_use.tool_name.clone());
    entry.call_id = item["id"]
        .as_str()
        .or_else(|| item["call_id"].as_str())
        .map(str::to_owned);
    entry.managed_tool_use = Some(tool_use);

    let output_fields: &[&str] = if item["aggregatedOutput"].is_string() {
        &["aggregatedOutput", "error"]
    } else if item["aggregated_output"].is_string() {
        &["aggregated_output", "error"]
    } else {
        &["stdout", "stderr", "error"]
    };
    let result = output_fields
        .iter()
        .filter_map(|key| item.get(*key).filter(|value| !value.is_null()))
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string())
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();
    if result.is_empty() {
        return vec![entry];
    }
    let failed = matches!(
        item["status"].as_str(),
        Some("failed" | "declined" | "interrupted")
    ) || item["exitCode"]
        .as_i64()
        .or_else(|| item["exit_code"].as_i64())
        .is_some_and(|code| code != 0)
        || !item["error"].is_null();
    let result = TimelineItem {
        // Preserve the trailing rollout offset used for native ordering.
        item_id: format!("result:{}", entry.item_id),
        kind: "tool_result".into(),
        role: "tool".into(),
        status: if failed {
            Some("error".into())
        } else {
            entry.status.clone()
        },
        content_preview: result.join("\n"),
        managed_tool_use: None,
        ..entry.clone()
    };
    vec![entry, result]
}
