use super::TurnTimelineService;
use crate::{
    AgentBinding,
    client_contract::{native_history::NativeHistory, raw_transcripts::*},
    timeline_cache::{Backfill, CacheKey, PageTurn, page_turn, source_key},
};
use pontia_storage_sqlite::models::turns::TurnRow;
use sha2::{Digest, Sha256};

impl TurnTimelineService {
    pub(super) async fn cached_read<F>(
        &self,
        binding: &AgentBinding,
        turns: &[TurnRow],
        selected: &[&TurnRow],
        history: &dyn NativeHistory,
        checkpoints: bool,
        native: impl FnOnce() -> F,
    ) -> std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>
    where
        F: std::future::Future<
                Output = std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>,
            >,
    {
        let Some(store) = &self.events.cache else {
            return native().await;
        };
        let Some(data) = self.clients.data(&binding.client_type) else {
            return native().await;
        };
        let Some(policy) = data.timeline_cache_policy() else {
            return native().await;
        };
        if checkpoints && !policy.supports_session_checkpoints {
            return native().await;
        }
        let indexes: Vec<_> = if turns.is_empty() {
            vec![0]
        } else {
            selected
                .iter()
                .filter_map(|t| turns.iter().position(|r| r.turn_id == t.turn_id))
                .collect()
        };
        let page: Option<Vec<_>> = indexes
            .iter()
            .map(|i| page_turn(turns, *i, checkpoints))
            .collect();
        let source = source_key(self.history_source, checkpoints);
        let guard = store
            .flight(&binding.session_id, source_key(self.history_source, false))
            .await;
        let Ok((generation, revision, exited)) = store.revision(&binding.session_id).await else {
            return native().await;
        };
        let exact = history.requires_exact_cache_page();
        if exact && ((!exited && !policy.read_while_online) || page.is_none()) {
            drop(guard);
            return native().await;
        }
        let validator = if let Some(probe) = history.cache_fingerprint() {
            let Some(fingerprint) = probe.await else {
                drop(guard);
                return native().await;
            };
            Some(fingerprint)
        } else {
            match data.timeline_cache_validator(&binding.into(), self.history_source) {
                Ok(validator) => validator,
                Err(
                    error @ (pontia_core::Error::Unavailable(_)
                    | pontia_core::Error::Unsupported(_)),
                ) => {
                    return Err(error.into());
                }
                Err(_) => return native().await,
            }
        };

        let key = CacheKey {
            generation: if policy.preserve_pre_compaction {
                generation
            } else {
                0
            },
            revision,
            binding,
            source,
            version: policy.render_version,
            validator,
        };
        let signature = exact.then(|| selection_signature(turns, &indexes));
        let exact_page = signature.as_deref().map(|signature| PageTurn {
            id: selected.first().map_or("", |turn| &turn.turn_id),
            head: Some(signature),
            tail: None,
            previous_tail: None,
            next_turn: None,
        });
        let lookup = exact_page
            .as_ref()
            .map(std::slice::from_ref)
            .or(page.as_deref());
        if (exited || policy.read_while_online)
            && let Some(page) = lookup
            && !page.is_empty()
        {
            match store.read(&key, page).await {
                Ok(Some(items)) => return Ok(items),
                Ok(None) => (),
                Err(error) => tracing::warn!(%error,"timeline cache read failed"),
            }
        }
        let result = native().await?;
        store
            .touch(
                &binding.session_id,
                time::OffsetDateTime::now_utc().unix_timestamp(),
            )
            .await;
        if let Some(page) = exact_page {
            if let Err(error) = store.write_exact_page(&key, &page, &result).await {
                tracing::warn!(%error, "timeline page cache write failed");
            }
            return Ok(result);
        }
        let fill = Backfill {
            store,
            key,
            turns,
            history,
            checkpoints,
        };
        if policy.backfill_all_turns_on_miss {
            fill.fill(0..turns.len().max(1)).await
        } else {
            fill.fill(indexes.into_iter()).await
        }
        Ok(result)
    }
}

// A checkpoint can change ownership when an unselected Turn boundary moves.
fn selection_signature(turns: &[TurnRow], indexes: &[usize]) -> String {
    let mut hash = Sha256::new();
    serde_json::to_writer(&mut hash, indexes).expect("serializable indexes");
    for turn in turns {
        serde_json::to_writer(
            &mut hash,
            &(&turn.turn_id, &turn.head_cursor, &turn.tail_cursor),
        )
        .expect("serializable boundaries");
    }
    format!("{:x}", hash.finalize())
}
