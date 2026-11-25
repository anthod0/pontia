use super::{CacheKey, PageTurn, TimelineCache};
use crate::{
    AgentBindingService, EventIngestService,
    client_contract::{
        native_history::{HistorySource, NativeHistory},
        raw_transcripts::*,
    },
    clients::ClientRegistry,
};
use pontia_core::Result;
use pontia_storage_sqlite::{models::turns::TurnRow, repositories::turns::SqliteTurnRepository};

pub(crate) fn page_turn<'a>(
    turns: &'a [TurnRow],
    i: usize,
    checkpoints: bool,
) -> Option<PageTurn<'a>> {
    if turns.is_empty() {
        return Some(PageTurn {
            id: "",
            head: None,
            tail: None,
            previous_tail: None,
            next_turn: None,
        });
    }
    let turn = &turns[i];
    if !matches!(turn.state.as_str(), "completed" | "failed" | "interrupted") {
        return None;
    }
    Some(PageTurn {
        id: &turn.turn_id,
        head: Some(turn.head_cursor.as_deref()?),
        tail: Some(turn.tail_cursor.as_deref()?),
        previous_tail: if checkpoints {
            turns[..i]
                .iter()
                .rev()
                .find_map(|t| t.tail_cursor.as_deref())
        } else {
            None
        },
        next_turn: if checkpoints {
            turns.get(i + 1).map(|t| t.turn_id.as_str())
        } else {
            None
        },
    })
}

pub(crate) struct Backfill<'a> {
    pub(crate) store: &'a TimelineCache,
    pub(crate) key: CacheKey<'a>,
    pub(crate) turns: &'a [TurnRow],
    pub(crate) history: &'a dyn NativeHistory,
    pub(crate) checkpoints: bool,
}

impl Backfill<'_> {
    pub(crate) async fn fill(&self, indexes: impl Iterator<Item = usize>) {
        let native_turns: Vec<_> = self
            .turns
            .iter()
            .map(|t| SessionTimelineTurn {
                turn_id: t.turn_id.clone(),
                head_cursor: t.head_cursor.clone(),
                tail_cursor: t.tail_cursor.clone(),
            })
            .collect();
        for i in indexes {
            let Some(turn) = page_turn(self.turns, i, self.checkpoints) else {
                continue;
            };
            let result = if self.checkpoints {
                self.history
                    .read_session_timeline(SessionTimelineSelection {
                        turns: native_turns.clone(),
                        selected_turn_ids: self
                            .turns
                            .get(i)
                            .map(|t| t.turn_id.clone())
                            .into_iter()
                            .collect(),
                    })
                    .await
            } else if self.turns.is_empty() {
                self.history.read_ranges(Vec::new()).await
            } else {
                self.history
                    .read_ranges(vec![TurnTimelineRange {
                        turn_id: turn.id.into(),
                        head_cursor: turn.head.unwrap().into(),
                        tail_cursor: turn.tail.map(str::to_owned),
                        is_first_session_turn: i == 0,
                    }])
                    .await
            };
            match result {
                Ok(items) => {
                    if let Err(error) = self.store.write(&self.key, &turn, &items).await {
                        tracing::warn!(%error, "timeline cache write failed");
                        break;
                    }
                }
                Err(error) => {
                    if matches!(error, TurnTimelineReadError::InvalidRange { .. })
                        && let Err(error) = self.store.discard(&self.key, turn.id).await
                    {
                        tracing::warn!(%error,"could not discard invalid cached Turn");
                    }
                    tracing::debug!(%error, "terminal Turn unavailable for timeline cache")
                }
            }
        }
    }
}

pub(crate) fn source_key(source: Option<HistorySource>, checkpoints: bool) -> &'static str {
    match (source, checkpoints) {
        (None, false) => "default",
        (None, true) => "default/session",
        (Some(HistorySource::Native), false) => "native",
        (Some(HistorySource::Native), true) => "native/session",
        (Some(HistorySource::File), false) => "file",
        (Some(HistorySource::File), true) => "file/session",
    }
}

#[derive(Clone)]
pub struct TimelineCacheWriter {
    store: TimelineCache,
    clients: ClientRegistry,
}

impl EventIngestService {
    pub fn timeline_cache_writer(&self) -> Option<TimelineCacheWriter> {
        Some(TimelineCacheWriter {
            store: self.cache.clone()?,
            clients: self.clients(),
        })
    }
}

impl TimelineCacheWriter {
    /// Flush a resident snapshot, or only the completed Turn and its adjacent checkpoint boundary.
    pub async fn flush(&self, session: &str, finished_turn: Option<&str>) {
        if let Err(error) = self.write_snapshot(session, finished_turn, None).await {
            tracing::warn!(%error,session,"resident timeline cache write failed");
        }
    }

    /// Cache all terminal ranges together: a new SDK fingerprint invalidates the
    /// entire previous snapshot within the current compaction generation.
    pub async fn flush_native_ranges(&self, session: &str) {
        if let Err(error) = self
            .write_snapshot(session, None, Some(HistorySource::Native))
            .await
        {
            tracing::warn!(%error,session,"native timeline cache write failed");
        }
    }

    async fn write_snapshot(
        &self,
        session: &str,
        finished_turn: Option<&str>,
        history_source: Option<HistorySource>,
    ) -> Result<()> {
        let db = &self.store.0.control;
        let Some(binding) = AgentBindingService::new(db.clone())
            .binding_for_session(session)
            .await?
        else {
            return Ok(());
        };
        let Some(data) = self.clients.data(&binding.client_type) else {
            return Ok(());
        };
        let Some(policy) = data.timeline_cache_policy() else {
            return Ok(());
        };
        let _guard = self
            .store
            .flight(session, source_key(history_source, false))
            .await;
        let (generation, revision, _) = self.store.revision(session).await?;
        let turns = SqliteTurnRepository::new(db.clone())
            .list_turns(session)
            .await?;
        let request = AgentBindingResolveRequest::from(&binding);
        let (history, session_checkpoints, validator) = if history_source.is_some() {
            let Some(history) = data.native_history(request.clone(), None) else {
                return Ok(());
            };
            let Some(probe) = history.cache_fingerprint() else {
                return Ok(());
            };
            let Some(fingerprint) = probe.await else {
                return Ok(());
            };
            (history, false, Some(fingerprint))
        } else {
            let Some(resident) = data.resident_history(request.clone()) else {
                return Ok(());
            };
            let validator = data.timeline_cache_validator(&request, None)?;
            (resident.history, resident.session_checkpoints, validator)
        };
        for checkpoints in [false, true]
            .into_iter()
            .filter(|value| !*value || session_checkpoints)
        {
            let source = source_key(history_source, checkpoints);
            let range = match finished_turn {
                Some(id) => {
                    let Some(i) = turns.iter().position(|t| t.turn_id == id) else {
                        return Ok(());
                    };
                    (if checkpoints { i.saturating_sub(1) } else { i })..i + 1
                }
                None => 0..turns.len().max(1),
            };
            let key = CacheKey {
                binding: &binding,
                version: policy.render_version,
                source,
                revision,
                generation: if policy.preserve_pre_compaction {
                    generation
                } else {
                    0
                },
                validator: validator.clone(),
            };
            Backfill {
                store: &self.store,
                key,
                turns: &turns,
                history: history.as_ref(),
                checkpoints,
            }
            .fill(range)
            .await;
        }
        Ok(())
    }
}
