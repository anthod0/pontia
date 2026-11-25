use super::{FORMAT_VERSION, RETENTION, TimelineCache, codec, now};
use crate::{AgentBinding, client_contract::raw_transcripts::TurnTimelineItem};
use pontia_core::{Error, Result};
use sqlx::Row;

pub(crate) struct CacheKey<'a> {
    pub binding: &'a AgentBinding,
    pub source: &'a str,
    pub version: i64,
    pub validator: Option<String>,
    pub generation: i64,
    pub revision: i64,
}

#[derive(serde::Serialize)]
pub(crate) struct PageTurn<'a> {
    pub id: &'a str,
    pub head: Option<&'a str>,
    pub tail: Option<&'a str>,
    pub previous_tail: Option<&'a str>,
    pub next_turn: Option<&'a str>,
}

impl TimelineCache {
    pub(crate) async fn read(
        &self,
        key: &CacheKey<'_>,
        turns: &[PageTurn<'_>],
    ) -> Result<Option<Vec<TurnTimelineItem>>> {
        let id = &key.binding.session_id;
        let db = self.db().await?;
        let (protected, updated) = self.activity(id).await?;
        let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
        let session: Option<(i64, bool)> = sqlx::query_as(
            "SELECT last_active_at, EXISTS (
                SELECT 1 FROM cached_sources WHERE session_id=?1 AND source_key=?2
                  AND binding_id=?3 AND native_key=?4 AND format_version=?5
                  AND render_version=?6 AND validator IS ?7 AND generation=?8)
             FROM cached_sessions WHERE session_id=?1",
        )
        .bind(id)
        .bind(key.source)
        .bind(&key.binding.id)
        .bind(&key.binding.client_session_key)
        .bind(FORMAT_VERSION)
        .bind(key.version)
        .bind(&key.validator)
        .bind(key.generation)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((active, valid_source)) = session else {
            return Ok(None);
        };
        if !protected && active.max(updated) < now() - RETENTION {
            sqlx::query("DELETE FROM cached_sessions WHERE session_id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(None);
        }
        if !valid_source {
            return Ok(None);
        }
        let rows = sqlx::query(
            "SELECT t.turn_id, t.encoding, t.payload,
                    (SELECT json_group_array(json_array(position, item_json) ORDER BY position)
                     FROM cached_checkpoints c WHERE (c.session_id,c.source_key,c.generation,c.turn_id)
                     = (t.session_id,t.source_key,t.generation,t.turn_id)) AS checkpoints
             FROM json_each(?) requested
             JOIN cached_turns t ON t.turn_id = requested.value ->> 'id'
             WHERE t.session_id=? AND t.source_key=? AND t.generation=? AND t.revision=?
               AND t.head IS (requested.value ->> 'head') AND t.tail IS (requested.value ->> 'tail')
               AND t.previous_tail IS (requested.value ->> 'previous_tail')
               AND t.next_turn IS (requested.value ->> 'next_turn')
             ORDER BY requested.key"
        )
        .bind(serde_json::to_string(turns)?)
        .bind(id).bind(key.source).bind(key.generation).bind(key.revision)
        .fetch_all(&mut *tx).await?;
        if rows.len() != turns.len() {
            return Ok(None);
        }
        let mut items = Vec::new();
        for row in rows {
            let turn_id: &str = row.try_get("turn_id")?;
            match decode_page(
                turn_id,
                row.try_get("encoding")?,
                row.try_get("payload")?,
                row.try_get("checkpoints")?,
            ) {
                Ok(entries) => items.extend(entries),
                Err(error) => {
                    sqlx::query("DELETE FROM cached_turns WHERE session_id=? AND source_key=? AND generation=? AND turn_id=?")
                        .bind(id).bind(key.source).bind(key.generation).bind(turn_id).execute(&mut *tx).await?;
                    tx.commit().await?;
                    return Err(error);
                }
            }
        }
        sqlx::query(
            "UPDATE cached_sessions SET last_active_at=max(last_active_at,?) WHERE session_id=?",
        )
        .bind(now().max(updated))
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(items))
    }

    pub(crate) async fn discard(&self, key: &CacheKey<'_>, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM cached_turns WHERE session_id=? AND source_key=? AND generation=? AND turn_id=?")
            .bind(&key.binding.session_id).bind(key.source).bind(key.generation).bind(id).execute(self.db().await?).await?;
        Ok(())
    }

    pub(crate) async fn write(
        &self,
        key: &CacheKey<'_>,
        turn: &PageTurn<'_>,
        items: &[TurnTimelineItem],
    ) -> Result<()> {
        let owned: Vec<_> = items
            .iter()
            .filter(|i| i.turn_id.is_some())
            .map(|i| &i.item)
            .collect();
        let valid = items.iter().all(|entry| match &entry.turn_id {
            Some(id) => id == turn.id,
            None => entry.item.kind == "context_compaction",
        });
        let (encoding, payload) = codec::encode(&owned)?;
        if !valid {
            self.discard(key, turn.id).await?;
            return Ok(());
        }
        self.persist(key, turn, encoding, payload, items).await
    }

    pub(crate) async fn write_exact_page(
        &self,
        key: &CacheKey<'_>,
        page: &PageTurn<'_>,
        items: &[TurnTimelineItem],
    ) -> Result<()> {
        let (encoding, payload) = codec::encode(items)?;
        self.persist(key, page, EXACT_PAGE_ENCODING + encoding, payload, &[])
            .await
    }

    async fn persist(
        &self,
        key: &CacheKey<'_>,
        turn: &PageTurn<'_>,
        encoding: i64,
        payload: Vec<u8>,
        items: &[TurnTimelineItem],
    ) -> Result<()> {
        if payload.len() > 8 * 1024 * 1024 {
            return self.discard(key, turn.id).await;
        }
        let id = &key.binding.session_id;
        let mut tx = self.db().await?.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM cached_sources WHERE session_id=? AND source_key=? AND (binding_id!=? OR native_key!=? OR format_version!=? OR render_version!=? OR (generation=? AND validator IS NOT ?))")
            .bind(id).bind(key.source).bind(&key.binding.id).bind(&key.binding.client_session_key).bind(FORMAT_VERSION).bind(key.version).bind(key.generation).bind(&key.validator).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO cached_sessions VALUES(?,?) ON CONFLICT(session_id) DO UPDATE SET last_active_at=max(last_active_at,excluded.last_active_at)")
            .bind(id).bind(now()).execute(&mut *tx).await?;
        sqlx::query("INSERT OR IGNORE INTO cached_sources VALUES(?,?,?,?,?,?,?,?)")
            .bind(id)
            .bind(key.source)
            .bind(key.generation)
            .bind(&key.binding.id)
            .bind(&key.binding.client_session_key)
            .bind(FORMAT_VERSION)
            .bind(key.version)
            .bind(&key.validator)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT OR REPLACE INTO cached_turns VALUES(?,?,?,?,?,?,?,?,?,?,?)")
            .bind(id)
            .bind(key.source)
            .bind(key.generation)
            .bind(turn.id)
            .bind(key.revision)
            .bind(turn.head)
            .bind(turn.tail)
            .bind(turn.previous_tail)
            .bind(turn.next_turn)
            .bind(encoding)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
        for (pos, entry) in items
            .iter()
            .enumerate()
            .filter(|(_, i)| i.turn_id.is_none())
        {
            sqlx::query("INSERT INTO cached_checkpoints VALUES(?,?,?,?,?,?)")
                .bind(id)
                .bind(key.source)
                .bind(key.generation)
                .bind(turn.id)
                .bind(pos as i64)
                .bind(serde_json::to_string(&entry.item)?)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

const EXACT_PAGE_ENCODING: i64 = 2;

fn decode_page(
    turn: &str,
    encoding: i64,
    payload: &[u8],
    checkpoints: &str,
) -> Result<Vec<TurnTimelineItem>> {
    if encoding >= EXACT_PAGE_ENCODING {
        return codec::decode(encoding - EXACT_PAGE_ENCODING, payload);
    }
    let mut entries: Vec<_> = codec::decode(encoding, payload)?
        .into_iter()
        .map(|item| TurnTimelineItem {
            turn_id: Some(turn.into()),
            item,
        })
        .collect();
    for (pos, json) in serde_json::from_str::<Vec<(usize, String)>>(checkpoints)? {
        if pos > entries.len() {
            return Err(Error::Domain("invalid cached checkpoint position".into()));
        }
        entries.insert(
            pos,
            TurnTimelineItem {
                turn_id: None,
                item: serde_json::from_str(&json)?,
            },
        );
    }
    Ok(entries)
}
