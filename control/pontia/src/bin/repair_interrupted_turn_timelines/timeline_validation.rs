use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use pontia_agent_clients::pi::raw_transcripts::PiJsonlV2Cursor;
use serde_json::Value;

pub(super) fn required<'a>(
    value: &'a Option<String>,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<&'a str> {
    match value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Some(value),
        None => {
            errors.push(format!("missing {name}"));
            None
        }
    }
}

pub(super) fn locate_entry_line_end(path: &Path, terminal_leaf_id: &str) -> Result<usize, String> {
    let file = File::open(path).map_err(|error| format!("source open failed: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut offset = 0usize;
    let mut matches = Vec::new();

    loop {
        line.clear();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("source read failed: {error}"))?;
        if bytes == 0 {
            break;
        }
        offset = offset
            .checked_add(bytes)
            .ok_or_else(|| "source offset overflow".to_string())?;
        let value: Value = serde_json::from_slice(&line).map_err(|error| {
            format!("source contains invalid JSONL before terminal entry: {error}")
        })?;
        if value.get("id").and_then(Value::as_str) == Some(terminal_leaf_id) {
            matches.push(offset);
        }
    }

    match matches.as_slice() {
        [offset] => Ok(*offset),
        [] => Err("terminal_leaf_id was not found in the resolved JSONL source".to_string()),
        _ => Err("terminal_leaf_id is ambiguous in the resolved JSONL source".to_string()),
    }
}

pub(super) fn validate_offset(
    offset: usize,
    binding_id: &str,
    head_cursor: Option<&str>,
    next_head_cursor: Option<&str>,
    errors: &mut Vec<String>,
) {
    match head_cursor {
        Some(cursor) => match PiJsonlV2Cursor::decode(cursor, binding_id) {
            Ok(head) if offset > head.byte_offset => {}
            Ok(_) => errors.push("terminal entry precedes the turn head boundary".to_string()),
            Err(error) => errors.push(format!("turn head cursor is invalid: {error}")),
        },
        None => errors.push("turn has no head cursor".to_string()),
    }
    if let Some(cursor) = next_head_cursor {
        match PiJsonlV2Cursor::decode(cursor, binding_id) {
            Ok(next_head) if offset <= next_head.byte_offset => {}
            Ok(_) => errors.push("terminal entry is after the next turn head boundary".to_string()),
            Err(error) => errors.push(format!("next turn head cursor is invalid: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::locate_entry_line_end;

    #[test]
    fn locates_the_exact_terminal_entry_line_end() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("session.jsonl");
        let first = b"{\"type\":\"message\",\"id\":\"leaf\"}\n";
        let second = b"{\"type\":\"message\",\"id\":\"terminal_leaf\"}\n";
        fs::write(&source, [first.as_slice(), second.as_slice()].concat()).unwrap();

        assert_eq!(
            locate_entry_line_end(&source, "terminal_leaf").unwrap(),
            first.len() + second.len()
        );
    }

    #[test]
    fn rejects_duplicate_terminal_entry_ids() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("session.jsonl");
        fs::write(
            &source,
            concat!(
                "{\"type\":\"message\",\"id\":\"duplicate\"}\n",
                "{\"type\":\"message\",\"id\":\"duplicate\"}\n"
            ),
        )
        .unwrap();

        assert!(
            locate_entry_line_end(&source, "duplicate")
                .unwrap_err()
                .contains("ambiguous")
        );
    }
}
