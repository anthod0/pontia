use pontia_core::Error;

use crate::raw_transcripts::{
    ResolvedAgentBinding, TurnTimelineRange, TurnTimelineReadError, TurnTimelineReadRequest,
};

use super::timeline::{PiJsonlV2CursorDecodeError, PiNativeTurnEntry, read_native_turn_ranges};
use super::{PiJsonlV2Cursor, PiTimelineAdapter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiTurnUserEntryResolveRequest {
    pub source: ResolvedAgentBinding,
    pub session_id: String,
    pub turn_session_id: String,
    pub turn_id: String,
    pub is_first_session_turn: bool,
    pub head_cursor: Option<String>,
    pub tail_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPiUserEntry {
    pub entry_id: String,
}

#[derive(Debug)]
pub enum PiTurnUserEntryResolveError {
    SessionMismatch {
        turn_id: String,
    },
    BoundaryMissing {
        turn_id: String,
        boundary: &'static str,
    },
    SourceUnsupported,
    SourceUnavailable,
    BindingStale {
        turn_id: String,
    },
    InvalidRange {
        turn_id: String,
    },
    UserEntryMissing {
        turn_id: String,
    },
    UserEntryAmbiguous {
        turn_id: String,
    },
    Inner(Error),
}

impl std::fmt::Display for PiTurnUserEntryResolveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionMismatch { turn_id } => {
                write!(formatter, "Turn {turn_id} belongs to another Session")
            }
            Self::BoundaryMissing { turn_id, boundary } => {
                write!(
                    formatter,
                    "Turn {turn_id} is missing its {boundary} boundary"
                )
            }
            Self::SourceUnsupported => {
                formatter.write_str("current transcript source is not supported by Pi")
            }
            Self::SourceUnavailable => formatter.write_str("Pi transcript source is unavailable"),
            Self::BindingStale { turn_id } => {
                write!(formatter, "Turn {turn_id} has a stale Pi binding")
            }
            Self::InvalidRange { turn_id } => {
                write!(formatter, "Turn {turn_id} has an invalid Pi timeline range")
            }
            Self::UserEntryMissing { turn_id } => {
                write!(formatter, "Turn {turn_id} has no Pi user-message entry")
            }
            Self::UserEntryAmbiguous { turn_id } => {
                write!(
                    formatter,
                    "Turn {turn_id} has multiple Pi user-message entries"
                )
            }
            Self::Inner(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for PiTurnUserEntryResolveError {}

pub trait PiTurnUserEntryResolver {
    fn resolve_user_entry(
        &self,
        request: PiTurnUserEntryResolveRequest,
    ) -> Result<ResolvedPiUserEntry, PiTurnUserEntryResolveError>;
}

impl PiTurnUserEntryResolver for PiTimelineAdapter {
    fn resolve_user_entry(
        &self,
        request: PiTurnUserEntryResolveRequest,
    ) -> Result<ResolvedPiUserEntry, PiTurnUserEntryResolveError> {
        if request.session_id != request.turn_session_id {
            return Err(PiTurnUserEntryResolveError::SessionMismatch {
                turn_id: request.turn_id,
            });
        }
        if request.source.client_type != "pi" || request.source.format != "pi-jsonl" {
            return Err(PiTurnUserEntryResolveError::SourceUnsupported);
        }
        let head_cursor =
            request
                .head_cursor
                .ok_or_else(|| PiTurnUserEntryResolveError::BoundaryMissing {
                    turn_id: request.turn_id.clone(),
                    boundary: "head",
                })?;
        let tail_cursor =
            request
                .tail_cursor
                .ok_or_else(|| PiTurnUserEntryResolveError::BoundaryMissing {
                    turn_id: request.turn_id.clone(),
                    boundary: "tail",
                })?;
        let turn_id = request.turn_id;
        validate_cursor(&head_cursor, &request.source.id, &turn_id)?;
        validate_cursor(&tail_cursor, &request.source.id, &turn_id)?;
        let entries = read_native_turn_ranges(TurnTimelineReadRequest {
            source: request.source,
            ranges: vec![TurnTimelineRange {
                turn_id: turn_id.clone(),
                is_first_session_turn: request.is_first_session_turn,
                head_cursor,
                tail_cursor: Some(tail_cursor),
            }],
        })
        .map_err(|error| classify_reader_error(error, &turn_id))?;
        let mut user_entries = entries.into_iter().filter(is_primary_user_message);
        let Some(entry) = user_entries.next() else {
            return Err(PiTurnUserEntryResolveError::UserEntryMissing { turn_id });
        };
        if user_entries.next().is_some() {
            return Err(PiTurnUserEntryResolveError::UserEntryAmbiguous { turn_id });
        }
        Ok(ResolvedPiUserEntry {
            entry_id: entry.entry_id,
        })
    }
}

fn is_primary_user_message(entry: &PiNativeTurnEntry) -> bool {
    entry.value.get("type").and_then(serde_json::Value::as_str) == Some("message")
        && entry
            .value
            .get("message")
            .and_then(|message| message.get("role"))
            .and_then(serde_json::Value::as_str)
            == Some("user")
}

fn validate_cursor(
    cursor: &str,
    binding_id: &str,
    turn_id: &str,
) -> Result<(), PiTurnUserEntryResolveError> {
    PiJsonlV2Cursor::decode_classified(cursor, binding_id)
        .map(|_| ())
        .map_err(|error| match error {
            PiJsonlV2CursorDecodeError::ScopeMismatch => {
                PiTurnUserEntryResolveError::BindingStale {
                    turn_id: turn_id.to_string(),
                }
            }
            PiJsonlV2CursorDecodeError::FormatMismatch
            | PiJsonlV2CursorDecodeError::InvalidByteOffset
            | PiJsonlV2CursorDecodeError::InvalidRelation => {
                PiTurnUserEntryResolveError::InvalidRange {
                    turn_id: turn_id.to_string(),
                }
            }
        })
}

fn classify_reader_error(
    error: TurnTimelineReadError,
    turn_id: &str,
) -> PiTurnUserEntryResolveError {
    match error {
        TurnTimelineReadError::InvalidRange { .. } => PiTurnUserEntryResolveError::InvalidRange {
            turn_id: turn_id.to_string(),
        },
        TurnTimelineReadError::Inner(Error::CapabilityUnavailable(_)) => {
            PiTurnUserEntryResolveError::SourceUnavailable
        }
        TurnTimelineReadError::Inner(error) => PiTurnUserEntryResolveError::Inner(error),
    }
}
