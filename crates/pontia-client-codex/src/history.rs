pub(crate) mod cursor;
pub(crate) mod files;
mod items;

use crate::{rollout::CodexRollout, runtime::CodexRuntime};
use pontia_agent_control::client_contract::{
    history::{RecoveredTurnHistory, TurnHistoryCandidate},
    native_history::{HistoryRead, HistorySelection, HistorySource, NativeHistory},
    raw_transcripts::{
        AgentBindingResolveRequest, AgentBindingResolver, SessionTimelineReadRequest,
        SessionTimelineSelection, SessionTimelineTurn, TimelineBoundaryCaptureKind,
        TurnTimelineItem, TurnTimelineRange, TurnTimelineReadError, TurnTimelineReadRequest,
        TurnTimelineReader,
    },
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};
use tokio::sync::OnceCell;

enum Snapshot {
    Native(Vec<Value>),
    File(
        pontia_agent_control::client_contract::raw_transcripts::ResolvedAgentBinding,
        Vec<String>,
    ),
}

pub(crate) struct CodexHistory {
    binding: AgentBindingResolveRequest,
    pool: Option<sqlx::SqlitePool>,
    snapshot: OnceCell<Snapshot>,
}

impl CodexHistory {
    pub fn new(binding: AgentBindingResolveRequest, pool: Option<sqlx::SqlitePool>) -> Self {
        Self {
            binding,
            pool,
            snapshot: OnceCell::new(),
        }
    }

    async fn runtime(&self) -> Result<Option<Arc<CodexRuntime>>> {
        let Some(pool) = &self.pool else {
            return Ok(None);
        };
        let root: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(metadata,'$.codex_control_root') FROM sessions WHERE session_id=?",
        )
        .bind(&self.binding.session_id)
        .fetch_optional(pool)
        .await?
        .flatten();
        match root {
            Some(root) => Ok(CodexRuntime::existing(Path::new(&root)).await),
            None => Ok(None),
        }
    }

    async fn snapshot(&self, source: Option<HistorySource>) -> Result<&Snapshot> {
        self.snapshot.get_or_try_init(|| async {
            let runtime = self.runtime().await?;
            let connection = match &runtime {
                Some(runtime) => runtime.connection().await.ok(),
                None => None,
            };
            if source != Some(HistorySource::File) {
                if let Some(connection) = connection {
                    let mut turns = Vec::new();
                    let mut cursor = None;
                    let mut seen = HashSet::new();
                    loop {
                        let mut params = json!({"threadId":self.binding.client_session_key,"limit":100,"sortDirection":"asc","itemsView":"full"});
                        if let Some(cursor) = cursor { params["cursor"] = cursor; }
                        let response = connection.call("thread/turns/list", params).await?;
                        let page = response["data"].as_array().ok_or_else(|| Error::Domain("Codex turns/list has no data".into()))?;
                        turns.extend(page.iter().cloned());
                        let next = &response["nextCursor"];
                        if next.is_null() { break; }
                        if !seen.insert(next.to_string()) { return Err(Error::Domain("Codex repeated history cursor".into())); }
                        cursor = Some(next.clone());
                    }
                    return Ok(Snapshot::Native(turns));
                }
                if source == Some(HistorySource::Native) {
                    return Err(Error::CapabilityUnavailable("source_unavailable: Codex app-server disconnected".into()));
                }
            }
            let home = match runtime {
                Some(runtime) => runtime.codex_home().to_path_buf(),
                None => match self.binding.client_session_file.as_deref().and_then(|path| path.ancestors().find(|ancestor| matches!(ancestor.file_name().and_then(|name| name.to_str()), Some("sessions" | "archived_sessions"))).and_then(Path::parent)) {
                    Some(home) => home.to_path_buf(),
                    None if self.binding.client_session_file.is_some() => self.binding.client_session_file.as_deref().and_then(Path::parent).expect("rollout parent").to_path_buf(),
                    None => crate::runtime::daemon::Endpoint::resolve()?.home,
                },
            };
            let path = files::current_path(&home, &self.binding.client_session_key, self.binding.client_session_file.as_deref()).await?;
            let mut binding = self.binding.clone();
            binding.client_session_file = Some(path);
            let resolved = CodexRollout.resolve(&binding)?;
            let mut turns = Vec::new();
            for (_, record) in files::records(&resolved.path)? {
                if let Some(id) = record.pointer("/payload/turn_id").and_then(Value::as_str)
                    && !turns.iter().any(|turn| turn == id) { turns.push(id.to_owned()); }
            }
            Ok(Snapshot::File(resolved, turns))
        }).await
    }

    fn associations(&self, turns: &[SessionTimelineTurn]) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        for turn in turns {
            let head = turn.head_cursor.as_ref().ok_or_else(|| {
                Error::Domain(format!(
                    "Codex Turn {} has no native boundary",
                    turn.turn_id
                ))
            })?;
            {
                let native =
                    cursor::decode(head, &self.binding, TimelineBoundaryCaptureKind::Head)?;
                if let Some(tail) = &turn.tail_cursor
                    && cursor::decode(tail, &self.binding, TimelineBoundaryCaptureKind::Tail)?
                        != native
                {
                    return Err(Error::Domain(
                        "Codex Turn boundary identity mismatch".into(),
                    ));
                }
                map.insert(native, turn.turn_id.clone());
            }
        }
        Ok(map)
    }
}

impl NativeHistory for CodexHistory {
    fn select_timeline(
        &self,
        turns: Vec<SessionTimelineTurn>,
        source: Option<HistorySource>,
    ) -> HistoryRead<'_, Result<Option<HistorySelection>>> {
        Box::pin(async move {
            let map = self.associations(&turns)?;
            let (source, ids) = match self.snapshot(source).await? {
                Snapshot::Native(turns) => (
                    HistorySource::Native,
                    turns
                        .iter()
                        .map(|turn| {
                            turn["id"]
                                .as_str()
                                .ok_or_else(|| Error::Domain("Codex Turn has no identity".into()))
                        })
                        .collect::<Result<Vec<_>>>()?,
                ),
                Snapshot::File(_, turns) => (
                    HistorySource::File,
                    turns.iter().map(String::as_str).collect(),
                ),
            };
            Ok(Some(HistorySelection {
                source,
                turn_ids: ids
                    .into_iter()
                    .filter_map(|id| map.get(id).cloned())
                    .collect(),
            }))
        })
    }

    fn read_session_timeline(
        &self,
        selection: SessionTimelineSelection,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>> {
        Box::pin(async move {
            let map = self.associations(&selection.turns)?;
            match self.snapshot(None).await? {
                Snapshot::Native(turns) => {
                    items::timeline(turns, &map, &selection.selected_turn_ids).map_err(Into::into)
                }
                Snapshot::File(source, _) => {
                    CodexRollout.read_session_timeline(SessionTimelineReadRequest {
                        source: source.clone(),
                        selection,
                    })
                }
            }
        })
    }

    fn read_ranges(
        &self,
        ranges: Vec<TurnTimelineRange>,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>> {
        Box::pin(async move {
            match self.snapshot(None).await? {
                Snapshot::File(source, _) => {
                    CodexRollout.read_turn_ranges(TurnTimelineReadRequest {
                        source: source.clone(),
                        ranges,
                    })
                }
                Snapshot::Native(_) => {
                    self.read_session_timeline(SessionTimelineSelection {
                        selected_turn_ids: ranges
                            .iter()
                            .map(|range| range.turn_id.clone())
                            .collect(),
                        turns: ranges
                            .into_iter()
                            .map(|range| SessionTimelineTurn {
                                turn_id: range.turn_id,
                                head_cursor: Some(range.head_cursor),
                                tail_cursor: range.tail_cursor,
                            })
                            .collect(),
                    })
                    .await
                }
            }
        })
    }

    fn recover(
        &self,
        turns: Vec<TurnHistoryCandidate>,
    ) -> HistoryRead<'_, Result<Vec<RecoveredTurnHistory>>> {
        Box::pin(async move {
            use pontia_agent_control::client_contract::{
                TopologyDiagnostic, TopologyResolution, TopologyResolveResult,
            };
            use pontia_core::domain::TurnState;
            let Some(pool) = &self.pool else {
                return Ok(Vec::new());
            };
            let mut recovered = Vec::new();
            for turn in turns {
                let terminal = matches!(
                    turn.state,
                    TurnState::Completed | TurnState::Failed | TurnState::Interrupted
                );
                if turn.head_cursor.is_some() && (!terminal || turn.tail_cursor.is_some()) {
                    continue;
                }
                let native: Option<String> = sqlx::query_scalar("SELECT client_turn_id FROM native_turn_bindings WHERE session_id=? AND turn_id=?")
                    .bind(&self.binding.session_id).bind(&turn.turn_id).fetch_optional(pool).await?;
                let native = native.ok_or_else(|| {
                    Error::Domain(format!(
                        "Codex Turn {} has no native association",
                        turn.turn_id
                    ))
                })?;
                recovered.push(RecoveredTurnHistory {
                    turn_id: turn.turn_id,
                    head_cursor: turn.head_cursor.is_none().then(|| {
                        cursor::encode(
                            &self.binding.id,
                            &self.binding.client_session_key,
                            &native,
                            TimelineBoundaryCaptureKind::Head,
                        )
                    }),
                    tail_cursor: (terminal && turn.tail_cursor.is_none()).then(|| {
                        cursor::encode(
                            &self.binding.id,
                            &self.binding.client_session_key,
                            &native,
                            TimelineBoundaryCaptureKind::Tail,
                        )
                    }),
                    topology: TopologyResolveResult {
                        resolution: TopologyResolution::Unknown,
                        diagnostic: TopologyDiagnostic::AdapterUnavailable,
                    },
                });
            }
            Ok(recovered)
        })
    }
    fn branch_target(&self, _range: TurnTimelineRange) -> HistoryRead<'_, Result<String>> {
        Box::pin(async {
            Err(Error::CapabilityUnavailable(
                "Codex branch control is unavailable".into(),
            ))
        })
    }
}
