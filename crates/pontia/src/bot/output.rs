use serde_json::Value;
use std::fmt::Write;

/// Keep all returned fields available in text mode, including sources and opaque IDs.
pub(super) fn plain_text(data: &Value) -> String {
    let mut output = String::new();
    fields(data, 0, &mut output);
    output
}

fn fields(value: &Value, indent: usize, output: &mut String) {
    match value {
        Value::Object(object) if !object.is_empty() => {
            for (key, value) in object {
                let label = key.replace('_', " ");
                write!(output, "{:indent$}{label}:", "").unwrap();
                field_value(value, indent, output);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for item in items {
                write!(output, "{:indent$}-", "").unwrap();
                field_value(item, indent, output);
            }
        }
        _ => {
            write!(output, "{:indent$}", "").unwrap();
            scalar(value, output);
            output.push('\n');
        }
    }
}

fn field_value(value: &Value, indent: usize, output: &mut String) {
    if value.as_object().is_some_and(|v| !v.is_empty())
        || value.as_array().is_some_and(|v| !v.is_empty())
        || value.as_str().is_some_and(|v| v.contains('\n'))
    {
        output.push('\n');
        if let Some(text) = value.as_str() {
            for line in text.split('\n') {
                writeln!(output, "{:width$}{line}", "", width = indent + 2).unwrap();
            }
        } else {
            fields(value, indent + 2, output);
        }
    } else {
        output.push(' ');
        scalar(value, output);
        output.push('\n');
    }
}

fn scalar(value: &Value, output: &mut String) {
    match value {
        Value::String(text) => output.push_str(text),
        Value::Null => output.push_str("none"),
        Value::Array(_) | Value::Object(_) => output.push_str("(empty)"),
        Value::Bool(value) => output.push_str(if *value { "yes" } else { "no" }),
        Value::Number(value) => write!(output, "{value}").unwrap(),
    }
}

pub(super) fn continuation(endpoint: &str, input: &Value, data: &Value) -> Option<String> {
    let (mut command, query) = if endpoint == "session-messages" {
        let next = data.get("next").filter(|next| !next.is_null())?;
        ("pontia bot session-messages".to_owned(), next.clone())
    } else if endpoint == "helper" && input["action"] == "check" {
        let next = data
            .get("messages")?
            .get("next")
            .filter(|next| !next.is_null())?;
        ("pontia bot helper check".to_owned(), next.clone())
    } else if endpoint == "memory" && input["action"] == "neighbors" {
        let cursor = data.get("next_cursor").filter(|cursor| !cursor.is_null())?;
        let mut next = input["input"].clone();
        next["after"] = cursor.clone();
        ("pontia bot memory neighbors".to_owned(), next)
    } else {
        return None;
    };
    for (key, value) in query.as_object().expect("continuation query is an object") {
        if value.is_null() {
            continue;
        }
        let flag = if key == "categories" {
            "category".to_owned()
        } else {
            key.replace('_', "-")
        };
        if let Some(values) = value.as_array() {
            for value in values {
                argument(&mut command, &flag, value);
            }
        } else {
            argument(&mut command, &flag, value);
        }
    }
    Some(command)
}

fn argument(command: &mut String, flag: &str, value: &Value) {
    let value = match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        _ => unreachable!("continuation arguments are strings or numbers"),
    };
    // Single-quote every value so opaque client locators remain literal shell arguments.
    write!(command, " --{flag}='{}'", value.replace('\'', "'\\''")).unwrap();
}
