use pontia_core::error::{Error, Result};

pub(super) fn validate_handle(handle: &str) -> Result<()> {
    let mut chars = handle.chars();
    if chars.next() != Some('@') {
        return Err(invalid_handle(handle));
    }
    let Some(first) = chars.next() else {
        return Err(invalid_handle(handle));
    };
    if !first.is_ascii_lowercase() {
        return Err(invalid_handle(handle));
    }
    let remaining: Vec<char> = chars.collect();
    if remaining.is_empty() || remaining.len() > 30 {
        return Err(invalid_handle(handle));
    }
    if !remaining
        .iter()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == '_' || *ch == '-')
    {
        return Err(invalid_handle(handle));
    }
    Ok(())
}

fn invalid_handle(handle: &str) -> Error {
    Error::Domain(format!(
        "Invalid session handle {handle}. Handle must match @[a-z][a-z0-9_-]{{1,31}}."
    ))
}
