use pontia_application::{LiveOutputItem, LiveOutputUpdate};
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

fn nonempty<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value[field]
        .as_str()
        .filter(|value| !value.trim().is_empty())
}
