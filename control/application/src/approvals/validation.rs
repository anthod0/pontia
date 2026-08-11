use pontia_core::error::{Error, Result};
use serde_json::{Map, Value};

use super::{MAX_APPROVAL_STRING_CHARS, MAX_PERMISSION_DIRECTORIES, MAX_PERMISSION_RULES};

pub(super) fn bounded_required<'a>(field: &str, value: &'a str) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::Domain(format!("{field} is required")));
    }
    if value.chars().count() > MAX_APPROVAL_STRING_CHARS {
        return Err(Error::Domain(format!(
            "{field} exceeds {MAX_APPROVAL_STRING_CHARS} characters"
        )));
    }
    Ok(value)
}

pub(super) fn valid_permission_suggestion(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let Some(kind) = object.get("type").and_then(Value::as_str) else {
        return false;
    };
    match kind {
        "addRules" | "replaceRules" | "removeRules" => {
            has_exact_keys(object, &["type", "rules", "behavior", "destination"])
                && valid_rules(object.get("rules"))
                && matches!(
                    object.get("behavior").and_then(Value::as_str),
                    Some("allow" | "deny" | "ask")
                )
                && valid_destination(object.get("destination"))
        }
        "setMode" => {
            has_exact_keys(object, &["type", "mode", "destination"])
                && matches!(
                    object.get("mode").and_then(Value::as_str),
                    Some(
                        "default"
                            | "auto"
                            | "acceptEdits"
                            | "dontAsk"
                            | "bypassPermissions"
                            | "plan"
                            | "manual"
                    )
                )
                && valid_destination(object.get("destination"))
        }
        "addDirectories" => {
            has_exact_keys(object, &["type", "directories", "destination"])
                && valid_directories(object.get("directories"))
                && valid_destination(object.get("destination"))
        }
        _ => false,
    }
}

fn valid_rules(value: Option<&Value>) -> bool {
    let Some(rules) = value.and_then(Value::as_array) else {
        return false;
    };
    !rules.is_empty()
        && rules.len() <= MAX_PERMISSION_RULES
        && rules.iter().all(|rule| {
            let Some(rule) = rule.as_object() else {
                return false;
            };
            let keys_valid = rule
                .keys()
                .all(|key| key == "toolName" || key == "ruleContent")
                && rule.contains_key("toolName");
            keys_valid
                && valid_bounded_string(rule.get("toolName"))
                && rule
                    .get("ruleContent")
                    .is_none_or(|value| valid_bounded_string(Some(value)))
        })
}

fn valid_directories(value: Option<&Value>) -> bool {
    let Some(directories) = value.and_then(Value::as_array) else {
        return false;
    };
    !directories.is_empty()
        && directories.len() <= MAX_PERMISSION_DIRECTORIES
        && directories
            .iter()
            .all(|directory| valid_bounded_string(Some(directory)))
}

fn valid_bounded_string(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str).is_some_and(|value| {
        !value.is_empty() && value.chars().count() <= MAX_APPROVAL_STRING_CHARS
    })
}

fn valid_destination(value: Option<&Value>) -> bool {
    matches!(
        value.and_then(Value::as_str),
        Some("userSettings" | "projectSettings" | "localSettings" | "session")
    )
}

fn has_exact_keys(object: &Map<String, Value>, keys: &[&str]) -> bool {
    object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
}
