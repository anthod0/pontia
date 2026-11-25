use super::TurnTimelineService;
use crate::{
    AgentBinding,
    client_contract::{native_history::NativeHistory, raw_transcripts::*},
    timeline_cache::{Backfill, CacheKey, page_turn, source_key},
};
use pontia_storage_sqlite::models::turns::TurnRow;

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
        let _guard = store
            .flight(&binding.session_id, source_key(self.history_source, false))
            .await;
        let validator = match data.timeline_cache_validator(&binding.into(), self.history_source) {
            Ok(validator) => validator,
            Err(error @ pontia_core::Error::CapabilityUnavailable(_)) => return Err(error.into()),
            Err(_) => return native().await,
        };

        let Ok((generation, revision, exited)) = store.revision(&binding.session_id).await else {
            return native().await;
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
        if (exited || policy.read_while_online)
            && let Some(page) = page
            && !page.is_empty()
        {
            match store.read(&key, &page).await {
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
