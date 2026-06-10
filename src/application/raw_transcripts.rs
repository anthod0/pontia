use super::*;
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBindingResolveRequest {
    pub id: String,
    pub session_id: String,
    pub client_type: String,
    pub launch_cwd: PathBuf,
    pub client_session_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentBinding {
    pub id: String,
    pub client_type: String,
    pub format: String,
    pub path: PathBuf,
    pub fingerprint: Option<String>,
}

pub trait AgentBindingResolver {
    fn client_type(&self) -> &'static str;
    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelinePageRequest {
    pub session_id: String,
    pub source: ResolvedAgentBinding,
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineItemDetailRequest {
    pub session_id: String,
    pub source: ResolvedAgentBinding,
    pub content_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineItem {
    pub item_id: String,
    pub kind: String,
    pub role: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub occurred_at: Option<String>,
    pub content_preview: String,
    pub content_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelinePage {
    pub session_id: String,
    pub binding_id: String,
    pub items: Vec<TimelineItem>,
    pub next_cursor: Option<String>,
    pub tail_cursor: Option<String>,
    pub has_more: bool,
    pub is_tail: bool,
    pub source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineItemDetailPage {
    pub binding_id: String,
    pub content_ref: String,
    pub content_type: String,
    pub text: String,
    pub size_bytes: usize,
}

pub trait RawTranscriptParser {
    fn client_type(&self) -> &'static str;
    fn format(&self) -> &'static str;
    fn timeline_page(&self, request: TimelinePageRequest) -> Result<TimelinePage>;
    fn timeline_item_detail(
        &self,
        request: TimelineItemDetailRequest,
    ) -> Result<TimelineItemDetailPage>;
}

#[derive(Debug, Clone)]
pub struct PiAgentBindingResolver {
    agent_dir: PathBuf,
}

impl PiAgentBindingResolver {
    pub fn new() -> Self {
        Self {
            agent_dir: default_pi_agent_dir(),
        }
    }

    pub fn with_agent_dir(agent_dir: PathBuf) -> Self {
        Self { agent_dir }
    }
}

impl Default for PiAgentBindingResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBindingResolver for PiAgentBindingResolver {
    fn client_type(&self) -> &'static str {
        "pi"
    }

    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding> {
        if request.client_type != self.client_type() {
            return Err(Error::CapabilityUnavailable(format!(
                "unsupported binding client_type {} for pi resolver",
                request.client_type
            )));
        }

        let session_dir = pi_session_dir(&self.agent_dir, &request.launch_cwd);
        let suffix = format!("_{}.jsonl", request.client_session_key);
        let mut matches = Vec::new();
        let entries = fs::read_dir(&session_dir).map_err(|err| {
            Error::CapabilityUnavailable(format!(
                "source_unavailable: pi session dir {} is unavailable: {err}",
                session_dir.display()
            ))
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(&suffix))
            {
                matches.push(path);
            }
        }
        matches.sort();

        let Some(path) = matches.pop() else {
            return Err(Error::CapabilityUnavailable(format!(
                "source_unavailable: pi session file for key {} not found under {}",
                request.client_session_key,
                session_dir.display()
            )));
        };

        Ok(ResolvedAgentBinding {
            id: request.id.clone(),
            client_type: request.client_type.clone(),
            format: "pi-jsonl".to_string(),
            path,
            fingerprint: None,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct PiJsonlParser;

impl PiJsonlParser {
    pub fn new() -> Self {
        Self
    }
}

impl RawTranscriptParser for PiJsonlParser {
    fn client_type(&self) -> &'static str {
        "pi"
    }

    fn format(&self) -> &'static str {
        "pi-jsonl"
    }

    fn timeline_page(&self, request: TimelinePageRequest) -> Result<TimelinePage> {
        if request.source.client_type != self.client_type()
            || request.source.format != self.format()
        {
            return Err(Error::CapabilityUnavailable(format!(
                "unsupported source {}/{} for pi jsonl parser",
                request.source.client_type, request.source.format
            )));
        }

        let source_id = source_id(&request.source);
        let cursor = decode_pi_cursor(request.cursor.as_deref(), &request.source.id)?;
        let bytes = fs::read(&request.source.path).map_err(|err| {
            Error::CapabilityUnavailable(format!(
                "source_unavailable: raw source {} is unavailable: {err}",
                request.source.path.display()
            ))
        })?;

        if cursor.offset > bytes.len() {
            return Err(Error::Domain(format!(
                "cursor_invalid: offset {} exceeds source length {}",
                cursor.offset,
                bytes.len()
            )));
        }

        let limit = request.limit.max(1);
        let mut items = Vec::new();
        let mut offset = 0usize;
        let mut next_position = CursorPosition {
            offset: cursor.offset,
            block_index: cursor.block_index,
        };
        let mut stopped_due_limit = false;

        for line in bytes.split_inclusive(|byte| *byte == b'\n') {
            let line_start = offset;
            let line_end = offset + line.len();
            offset = line_end;

            if line_end <= cursor.offset {
                continue;
            }
            if line_start < cursor.offset {
                continue;
            }

            let text = std::str::from_utf8(line)
                .map_err(|err| Error::Domain(format!("pi jsonl source is not utf-8: {err}")))?
                .trim_end_matches(['\r', '\n']);
            if text.trim().is_empty() {
                next_position = CursorPosition {
                    offset: line_end,
                    block_index: 0,
                };
                continue;
            }
            let entry: Value = serde_json::from_str(text)?;
            let produced = pi_entry_to_items(&entry, &request.source.id, line_start, line_end);
            let start_block = if line_start == cursor.offset {
                cursor.block_index
            } else {
                0
            };

            for (idx, item) in produced.into_iter().enumerate().skip(start_block) {
                if items.len() == limit {
                    stopped_due_limit = true;
                    next_position = CursorPosition {
                        offset: line_start,
                        block_index: idx,
                    };
                    break;
                }
                items.push(item);
                next_position = CursorPosition {
                    offset: line_start,
                    block_index: idx + 1,
                };
            }

            if stopped_due_limit {
                break;
            }

            next_position = CursorPosition {
                offset: line_end,
                block_index: 0,
            };
        }

        let has_unread_bytes = next_position.offset < bytes.len();
        let has_more = stopped_due_limit || has_unread_bytes;
        let cursor_token = encode_pi_cursor(&request.source.id, next_position);

        Ok(TimelinePage {
            session_id: request.session_id,
            binding_id: request.source.id,
            items,
            next_cursor: Some(cursor_token.clone()),
            tail_cursor: Some(cursor_token),
            has_more,
            is_tail: !has_more,
            source_id,
        })
    }

    fn timeline_item_detail(
        &self,
        request: TimelineItemDetailRequest,
    ) -> Result<TimelineItemDetailPage> {
        let detail_ref = decode_pi_content_ref(&request.content_ref, &request.source.id)?;
        let bytes = fs::read(&request.source.path)?;
        if detail_ref.start > detail_ref.end || detail_ref.end > bytes.len() {
            return Err(Error::Domain(
                "content_ref_invalid: byte range outside source".to_string(),
            ));
        }
        let line = std::str::from_utf8(&bytes[detail_ref.start..detail_ref.end])
            .map_err(|err| {
                Error::Domain(format!("content_ref_invalid: source is not utf-8: {err}"))
            })?
            .trim_end_matches(['\r', '\n']);
        let entry: Value = serde_json::from_str(line)?;
        let text = match detail_ref.kind.as_str() {
            "assistant_message" | "assistant_thinking" | "tool_call" => entry
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| content.get(detail_ref.block_index))
                .cloned()
                .unwrap_or(Value::Null),
            _ => entry.clone(),
        };
        let text = serde_json::to_string_pretty(&text)?;
        Ok(TimelineItemDetailPage {
            binding_id: request.source.id,
            content_ref: request.content_ref,
            content_type: "application/json".to_string(),
            size_bytes: text.len(),
            text,
        })
    }
}

pub async fn resolve_and_parse_timeline_page<R, P>(
    binding: &AgentBinding,
    resolver: &R,
    parser: &P,
    cursor: Option<String>,
    limit: usize,
) -> Result<TimelinePage>
where
    R: AgentBindingResolver,
    P: RawTranscriptParser,
{
    let source = resolver.resolve(&AgentBindingResolveRequest {
        id: binding.id.clone(),
        session_id: binding.session_id.clone(),
        client_type: binding.client_type.clone(),
        launch_cwd: PathBuf::from(&binding.launch_cwd),
        client_session_key: binding.client_session_key.clone(),
    })?;
    parser.timeline_page(TimelinePageRequest {
        session_id: binding.session_id.clone(),
        source,
        cursor,
        limit,
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CursorPosition {
    offset: usize,
    block_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContentRef {
    start: usize,
    end: usize,
    block_index: usize,
    kind: String,
}

fn default_pi_agent_dir() -> PathBuf {
    if let Ok(path) = std::env::var("PI_AGENT_DIR") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".pi").join("agent")
}

fn pi_session_dir(agent_dir: &std::path::Path, cwd: &std::path::Path) -> PathBuf {
    let resolved = cwd.to_string_lossy();
    let safe_path = resolved
        .trim_start_matches(['/', '\\'])
        .replace(['/', '\\', ':'], "-");
    agent_dir.join("sessions").join(format!("--{safe_path}--"))
}

fn pi_entry_to_items(
    entry: &Value,
    binding_id: &str,
    start: usize,
    end: usize,
) -> Vec<TimelineItem> {
    match entry.get("type").and_then(Value::as_str) {
        Some("message") => pi_message_entry_to_items(entry, binding_id, start, end),
        Some("model_change") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "model_change",
            "system",
            model_change_title(entry),
            None,
            model_change_title(entry).unwrap_or_default(),
        )],
        _ => Vec::new(),
    }
}

fn pi_message_entry_to_items(
    entry: &Value,
    binding_id: &str,
    start: usize,
    end: usize,
) -> Vec<TimelineItem> {
    let Some(message) = entry.get("message") else {
        return Vec::new();
    };
    match message.get("role").and_then(Value::as_str) {
        Some("user") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "user_message",
            "user",
            None,
            None,
            content_preview(message.get("content")),
        )],
        Some("assistant") => message
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .enumerate()
                    .filter_map(|(block_index, block)| {
                        assistant_block_item(entry, binding_id, start, end, block_index, block)
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Some("toolResult") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "tool_result",
            "tool",
            message
                .get("toolName")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            Some(
                if message.get("isError").and_then(Value::as_bool) == Some(true) {
                    "error".to_string()
                } else {
                    "completed".to_string()
                },
            ),
            content_preview(message.get("content")),
        )],
        Some("bashExecution") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "user_bash",
            "user",
            message
                .get("command")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            None,
            message
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )],
        Some("custom") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "custom_message",
            "system",
            message
                .get("customType")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            None,
            content_preview(message.get("content")),
        )],
        Some("branchSummary") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "branch_summary",
            "system",
            None,
            None,
            message
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )],
        Some("compactionSummary") => vec![timeline_item(
            binding_id,
            entry,
            start,
            end,
            0,
            "compaction_summary",
            "system",
            None,
            None,
            message
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )],
        _ => Vec::new(),
    }
}

fn assistant_block_item(
    entry: &Value,
    binding_id: &str,
    start: usize,
    end: usize,
    block_index: usize,
    block: &Value,
) -> Option<TimelineItem> {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => Some(timeline_item(
            binding_id,
            entry,
            start,
            end,
            block_index,
            "assistant_message",
            "assistant",
            None,
            None,
            block
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )),
        Some("thinking") => Some(timeline_item(
            binding_id,
            entry,
            start,
            end,
            block_index,
            "assistant_thinking",
            "assistant",
            None,
            None,
            block
                .get("thinking")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        )),
        Some("toolCall") => Some(timeline_item(
            binding_id,
            entry,
            start,
            end,
            block_index,
            "tool_call",
            "tool",
            block
                .get("name")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            Some("started".to_string()),
            tool_call_preview(block),
        )),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn timeline_item(
    binding_id: &str,
    entry: &Value,
    start: usize,
    end: usize,
    block_index: usize,
    kind: &str,
    role: &str,
    title: Option<String>,
    status: Option<String>,
    preview: String,
) -> TimelineItem {
    let entry_id = entry.get("id").and_then(Value::as_str).unwrap_or("unknown");
    TimelineItem {
        item_id: format!("pi:entry:{entry_id}:block:{block_index}"),
        kind: kind.to_string(),
        role: role.to_string(),
        title,
        status,
        occurred_at: entry
            .get("timestamp")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        content_preview: truncate_preview(&preview),
        content_ref: encode_pi_content_ref(binding_id, start, end, block_index, kind),
    }
}

fn content_preview(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| {
                block
                    .get("text")
                    .or_else(|| block.get("thinking"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn tool_call_preview(block: &Value) -> String {
    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
    let args = block.get("arguments").cloned().unwrap_or(Value::Null);
    format!("{name} {args}")
}

fn model_change_title(entry: &Value) -> Option<String> {
    let provider = entry.get("provider").and_then(Value::as_str);
    let model = entry.get("modelId").and_then(Value::as_str);
    match (provider, model) {
        (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
        (None, Some(model)) => Some(model.to_string()),
        _ => None,
    }
}

fn truncate_preview(text: &str) -> String {
    const MAX_CHARS: usize = 240;
    let mut chars = text.chars();
    let preview: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn source_id(source: &ResolvedAgentBinding) -> String {
    format!("{}:{}", source.client_type, source.path.display())
}

fn encode_pi_cursor(binding_id: &str, position: CursorPosition) -> String {
    format!(
        "pi-jsonl-v1:{binding_id}:{}:{}",
        position.offset, position.block_index
    )
}

fn decode_pi_cursor(cursor: Option<&str>, binding_id: &str) -> Result<CursorPosition> {
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

fn encode_pi_content_ref(
    binding_id: &str,
    start: usize,
    end: usize,
    block_index: usize,
    kind: &str,
) -> String {
    format!("pi-jsonl-ref-v1:{binding_id}:{start}:{end}:{block_index}:{kind}")
}

fn decode_pi_content_ref(content_ref: &str, binding_id: &str) -> Result<ContentRef> {
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
