use super::{RETENTION, TimelineCache, now};
use pontia_core::Result;
use std::{collections::HashMap, time::Duration};

const MAX_BYTES: i64 = 2 * 1024 * 1024 * 1024;

impl TimelineCache {
    pub(crate) async fn activity(&self, session: &str) -> Result<(bool, i64)> {
        Ok(sqlx::query_as("SELECT pinned_at IS NOT NULL OR state NOT IN ('exited','error'), unixepoch(updated_at) FROM sessions WHERE session_id=?")
            .bind(session).fetch_one(&self.0.control).await?)
    }

    pub(crate) async fn run(self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let mut ticks = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(30),
            Duration::from_secs(6 * 3600),
        );
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = shutdown.wait_for(|s| *s) => break,
                _ = ticks.tick() => {}
            }
            if let Err(error) = self.cleanup(now(), MAX_BYTES).await {
                tracing::warn!(%error,"timeline cache cleanup failed");
            }
        }
    }

    pub(super) async fn cleanup(&self, at: i64, quota: i64) -> Result<()> {
        let db = self.db().await?;
        let sessions: Vec<(String, bool, i64)> = sqlx::query_as("SELECT session_id, pinned_at IS NOT NULL OR state NOT IN ('exited','error'), unixepoch(updated_at) FROM sessions")
            .fetch_all(&self.0.control).await?;
        let policies: HashMap<_, _> = sessions
            .into_iter()
            .map(|(id, pinned, updated)| (id, (pinned, updated)))
            .collect();
        let rows: Vec<(String,i64,i64)> = sqlx::query_as(
            "SELECT session_id, last_active_at,
                (SELECT coalesce(sum(length(payload)),0) FROM cached_turns t WHERE t.session_id=s.session_id) +
                (SELECT coalesce(sum(length(CAST(item_json AS BLOB))),0) FROM cached_checkpoints c WHERE c.session_id=s.session_id)
             FROM cached_sessions s"
        ).fetch_all(db).await?;
        let mut total = rows.iter().map(|r| r.2).sum::<i64>();
        let mut candidates = Vec::new();
        for (id, active, size) in rows {
            let (protected, updated) = policies.get(&id).copied().unwrap_or_default();
            if updated > active {
                self.touch(&id, updated).await;
            }
            if !protected {
                candidates.push((active.max(updated), id, size));
            }
        }
        candidates.sort_unstable();
        let expired_bytes = candidates
            .iter()
            .take_while(|r| r.0 < at - RETENTION)
            .map(|r| r.2)
            .sum::<i64>();
        let over_quota = total - expired_bytes > quota;
        for batch in candidates.chunks(50) {
            let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
            for (active, id, size) in batch {
                if *active >= at - RETENTION
                    && (!over_quota || total <= quota * 4 / 5 || *active >= at - 600)
                {
                    break;
                }
                // A Session can be pinned, resumed or viewed while a sweep is in progress.
                let policy:Option<(bool,i64)>=sqlx::query_as(
                    "SELECT pinned_at IS NOT NULL OR state NOT IN ('exited','error'), unixepoch(updated_at)
                     FROM sessions WHERE session_id=?"
                ).bind(id).fetch_optional(&self.0.control).await?;
                if policy.is_some_and(|(protected, updated)| protected || updated > *active) {
                    continue;
                }
                let deleted = sqlx::query(
                    "DELETE FROM cached_sessions WHERE session_id=? AND last_active_at<=?",
                )
                .bind(id)
                .bind(active)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                if deleted > 0 {
                    total -= size;
                }
            }
            tx.commit().await?;
        }
        sqlx::query("PRAGMA incremental_vacuum(200)")
            .execute(db)
            .await?;
        Ok(())
    }
}
