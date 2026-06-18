use pontia_core::{Error, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CursorPosition {
    pub(super) offset: usize,
    pub(super) block_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ContentRef {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) block_index: usize,
    pub(super) kind: String,
}

pub(super) fn encode_pi_cursor(binding_id: &str, position: CursorPosition) -> String {
    format!(
        "pi-jsonl-v1:{binding_id}:{}:{}",
        position.offset, position.block_index
    )
}

pub(super) fn decode_pi_cursor(cursor: Option<&str>, binding_id: &str) -> Result<CursorPosition> {
    let Some(cursor) = cursor else {
        return Ok(CursorPosition::default());
    };
    let parts: Vec<_> = cursor.split(':').collect();
    if parts.len() != 4 || parts[0] != "pi-jsonl-v1" || parts[1] != binding_id {
        return Err(Error::Domain(
            "cursor_invalid: cursor scope mismatch".to_string(),
        ));
    }
    Ok(CursorPosition {
        offset: parts[2]
            .parse()
            .map_err(|_| Error::Domain("cursor_invalid: invalid offset".to_string()))?,
        block_index: parts[3]
            .parse()
            .map_err(|_| Error::Domain("cursor_invalid: invalid block index".to_string()))?,
    })
}

pub(super) fn encode_pi_content_ref(
    binding_id: &str,
    start: usize,
    end: usize,
    block_index: usize,
    kind: &str,
) -> String {
    format!("pi-jsonl-ref-v1:{binding_id}:{start}:{end}:{block_index}:{kind}")
}

pub(super) fn decode_pi_content_ref(content_ref: &str, binding_id: &str) -> Result<ContentRef> {
    let parts: Vec<_> = content_ref.split(':').collect();
    if parts.len() != 6 || parts[0] != "pi-jsonl-ref-v1" || parts[1] != binding_id {
        return Err(Error::Domain(
            "content_ref_invalid: content ref scope mismatch".to_string(),
        ));
    }
    Ok(ContentRef {
        start: parts[2]
            .parse()
            .map_err(|_| Error::Domain("content_ref_invalid: invalid start".to_string()))?,
        end: parts[3]
            .parse()
            .map_err(|_| Error::Domain("content_ref_invalid: invalid end".to_string()))?,
        block_index: parts[4]
            .parse()
            .map_err(|_| Error::Domain("content_ref_invalid: invalid block index".to_string()))?,
        kind: parts[5].to_string(),
    })
}
