use pontia_application::client_contract::raw_transcripts::{
    AgentBindingResolveRequest, TimelineBoundaryCaptureKind,
};
use pontia_core::{Error, Result};
use serde::Deserialize;

pub fn encode(
    binding: &str,
    thread: &str,
    turn: &str,
    kind: TimelineBoundaryCaptureKind,
) -> String {
    let relation = match kind {
        TimelineBoundaryCaptureKind::Head => "before",
        TimelineBoundaryCaptureKind::Tail => "after",
    };
    format!("codex-turn-v2:{binding}:{thread}:{turn}:{relation}")
}

pub fn decode(
    value: &str,
    binding: &AgentBindingResolveRequest,
    kind: TimelineBoundaryCaptureKind,
) -> Result<String> {
    let invalid = || Error::Domain("Invalid Codex Turn boundary".into());
    if let Some(rest) = value.strip_prefix("codex-turn-v2:") {
        let (id, rest) = rest.split_once(':').ok_or_else(invalid)?;
        let (thread, rest) = rest.split_once(':').ok_or_else(invalid)?;
        let (turn, relation) = rest.rsplit_once(':').ok_or_else(invalid)?;
        let expected = match kind {
            TimelineBoundaryCaptureKind::Head => "before",
            TimelineBoundaryCaptureKind::Tail => "after",
        };
        if id != binding.id
            || thread != binding.client_session_key
            || turn.is_empty()
            || relation != expected
        {
            return Err(invalid());
        }
        return Ok(turn.into());
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct V1 {
        version: u8,
        binding: String,
        native_turn: String,
        #[serde(rename = "offset")]
        _offset: u64,
        thread: Option<String>,
    }
    let old: V1 = serde_json::from_str(value)?;
    if old.version != 1
        || old.binding != binding.id
        || old.native_turn.is_empty()
        || old
            .thread
            .is_some_and(|thread| thread != binding.client_session_key)
    {
        return Err(invalid());
    }
    Ok(old.native_turn)
}
