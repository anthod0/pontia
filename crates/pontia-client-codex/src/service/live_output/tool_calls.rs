use pontia_application::{
    LiveOutputItem, LiveOutputUpdate, LiveToolOutcome, LiveToolPhase, LiveToolResult,
    LiveToolStatus,
};
use serde_json::{Value, json};

pub(super) struct ToolCall {
    id: String,
    name: String,
    arguments: Value,
}

impl ToolCall {
    pub fn into_item(self) -> LiveOutputItem {
        LiveOutputItem::ToolCall {
            item_id: self.id.clone(),
            call_id: self.id,
            tool_name: self.name,
            arguments: self.arguments,
            managed_tool_use: None,
            status: None,
            output: None,
            result: None,
        }
    }

    pub fn into_update(self) -> LiveOutputUpdate {
        LiveOutputUpdate::ToolCall {
            item_id: self.id.clone(),
            call_id: self.id,
            tool_name: self.name,
            arguments: self.arguments,
            managed_tool_use: None,
        }
    }
}

/// Only native invocation inputs belong here. In particular, never copy the
/// entire item: completed items also carry outputs, errors and execution state.
pub(super) fn from_native(item: &Value) -> Option<ToolCall> {
    let id = nonempty(item, "id")?;
    // Legacy history synthesizes IDs that cannot be correlated with notifications.
    if super::is_history_item_id(Some(id)) {
        return None;
    }
    let (name, arguments) = match item["type"].as_str()? {
        "commandExecution" => {
            let command = nonempty(item, "command")?;
            let mut arguments = json!({"command": command});
            if let Some(cwd) = item["cwd"].as_str() {
                arguments["cwd"] = json!(cwd);
            }
            ("commandExecution".into(), arguments)
        }
        "fileChange" => {
            let changes = item["changes"].as_array()?.iter().map(|change| {
                let kind = match change["kind"]["type"].as_str()? {
                    "add" => json!({"type":"add"}),
                    "delete" => json!({"type":"delete"}),
                    "update" => {
                        let mut kind = json!({"type":"update"});
                        if let Some(path) = change["kind"]["move_path"].as_str() {
                            kind["move_path"] = json!(path);
                        }
                        kind
                    }
                    _ => return None,
                };
                Some(json!({"path":change["path"].as_str()?, "kind":kind, "diff":change["diff"].as_str()?}))
            }).collect::<Option<Vec<_>>>()?;
            ("fileChange".into(), json!({"changes":changes}))
        }
        "mcpToolCall" => (
            format!("{}/{}", nonempty(item, "server")?, nonempty(item, "tool")?),
            item.get("arguments")?.clone(),
        ),
        "dynamicToolCall" => {
            let tool = nonempty(item, "tool")?;
            let name = match nonempty(item, "namespace") {
                Some(namespace) => format!("{namespace}/{tool}"),
                None => tool.into(),
            };
            (name, item.get("arguments")?.clone())
        }
        "sleep" => (
            "clock.sleep".into(),
            json!({"durationMs":item["durationMs"].as_u64()?}),
        ),
        "imageView" => ("imageView".into(), json!({"path":nonempty(item, "path")?})),
        "webSearch" => {
            let mut arguments = json!({});
            if let Some(query) = nonempty(item, "query") {
                arguments["query"] = json!(query);
            }
            let action = &item["action"];
            let fields: &[&str] = match action["type"].as_str() {
                Some("search") => &["query", "queries"],
                Some("openPage") => &["url"],
                Some("findInPage") => &["url", "pattern"],
                _ => &[],
            };
            if !fields.is_empty() {
                let mut input = json!({"type":action["type"]});
                for field in fields {
                    if let Some(value) = action.get(*field).filter(|value| !value.is_null()) {
                        input[*field] = value.clone();
                    }
                }
                arguments["action"] = input;
            }
            if arguments.as_object()?.is_empty() {
                return None;
            }
            ("webSearch".into(), arguments)
        }
        "collabAgentToolCall" => {
            let tool = nonempty(item, "tool")?;
            let mut arguments = json!({});
            match tool {
                // A spawned agent's receiver ID is an output, not a call input.
                "spawnAgent" => {}
                "sendInput" | "resumeAgent" | "wait" | "closeAgent" | "sendMessage"
                | "followupTask" | "interruptAgent" => {
                    if let Some(receivers) = item["receiverThreadIds"].as_array() {
                        if !receivers.iter().all(Value::is_string) {
                            return None;
                        }
                        arguments["receiverThreadIds"] = json!(receivers);
                    }
                }
                "listAgents" => {}
                _ => return None,
            }
            for field in ["prompt", "model", "reasoningEffort"] {
                if let Some(value) = item[field].as_str() {
                    arguments[field] = json!(value);
                }
            }
            (tool.into(), arguments)
        }
        _ => return None,
    };
    Some(ToolCall {
        id: id.into(),
        name,
        arguments,
    })
}

/// Execution facts are separate from immutable invocation inputs. A bare
/// completion proves an end, not success; nullable protocol fields mean absent.
pub(super) fn execution(item: &Value, method: Option<&str>) -> Vec<LiveOutputUpdate> {
    let Some(id) = nonempty(item, "id").filter(|id| !super::is_history_item_id(Some(id))) else {
        return vec![];
    };
    let kind = item["type"].as_str().unwrap_or("");
    let native = item["status"].as_str();
    let ended = method == Some("item/completed")
        || matches!(
            native,
            Some("completed" | "failed" | "declined" | "interrupted")
        );
    let outcome = if ended {
        match native {
            Some("declined") => Some(LiveToolOutcome::Rejected),
            Some("interrupted") => Some(LiveToolOutcome::Interrupted),
            Some("failed") => Some(LiveToolOutcome::Failed),
            _ => match kind {
                "commandExecution" => item["exitCode"].as_i64().map(|code| {
                    if code == 0 {
                        LiveToolOutcome::Succeeded
                    } else {
                        LiveToolOutcome::Failed
                    }
                }),
                "dynamicToolCall" => item["success"].as_bool().map(|ok| {
                    if ok {
                        LiveToolOutcome::Succeeded
                    } else {
                        LiveToolOutcome::Failed
                    }
                }),
                "mcpToolCall" if !item["error"].is_null() => Some(LiveToolOutcome::Failed),
                "mcpToolCall" | "fileChange" | "collabAgentToolCall"
                    if native == Some("completed") =>
                {
                    Some(LiveToolOutcome::Succeeded)
                }
                _ => None,
            },
        }
    } else {
        None
    };
    let mut updates = vec![];
    if ended || native == Some("inProgress") || method == Some("item/started") {
        updates.push(LiveOutputUpdate::ToolStatusUpdated {
            call_id: id.into(),
            status: LiveToolStatus {
                phase: if ended {
                    LiveToolPhase::Ended
                } else {
                    LiveToolPhase::Running
                },
                outcome,
            },
        });
    }
    if !ended
        && kind == "commandExecution"
        && let Some(text) = item["aggregatedOutput"].as_str()
    {
        updates.push(LiveOutputUpdate::ToolOutputUpdated {
            call_id: id.into(),
            text: text.into(),
            truncated: true,
        });
    }
    if ended {
        let field = match kind {
            "commandExecution" => "aggregatedOutput",
            "mcpToolCall" => "result",
            "dynamicToolCall" => "contentItems",
            "collabAgentToolCall" => "agentsStates",
            _ => "",
        };
        let mut content = item.get(field).filter(|v| !v.is_null()).cloned();
        if kind == "collabAgentToolCall" {
            let mut result = serde_json::Map::new();
            if let Some(value) = content.take() {
                result.insert("agentsStates".into(), value);
            }
            if let Some(value) = item.get("receiverThreadIds") {
                result.insert("receiverThreadIds".into(), value.clone());
            }
            if !result.is_empty() {
                content = Some(Value::Object(result));
            }
        }
        let error = item.get("error").filter(|v| !v.is_null()).cloned();
        if content.is_some() || error.is_some() {
            updates.push(LiveOutputUpdate::ToolResult {
                call_id: id.into(),
                result: LiveToolResult {
                    content,
                    error,
                    truncated: item["truncated"].as_bool().unwrap_or(false),
                },
            });
        }
    }
    updates
}

fn nonempty<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value[field]
        .as_str()
        .filter(|value| !value.trim().is_empty())
}
