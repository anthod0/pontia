use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use super::{
    ArchiveMemory, CreateMemory, Error, Memory, MemoryAuthor, Result, UpdateMemory,
    keywords::Keywords,
};

#[derive(Clone)]
pub(super) struct Store(pub SqlitePool);

impl Store {
    pub async fn create(
        &self,
        input: CreateMemory,
        keywords: Keywords,
        author: MemoryAuthor,
        now: &str,
    ) -> Result<Memory> {
        let mut tx = self.0.begin().await?;
        let rowid: i64 = sqlx::query_scalar(
            "INSERT INTO bot_memories (id, kind, content, keywords, workspace_id, session_id, importance, occurred_at, sources, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING rowid"
        )
        .bind(format!("mem_{}", uuid::Uuid::now_v7()))
        .bind(input.kind).bind(input.content).bind(sqlx::types::Json(&keywords.words))
        .bind(input.workspace_id).bind(input.session_id).bind(input.importance)
        .bind(input.occurred_at).bind(input.sources).bind(author).bind(now).bind(now)
        .fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO bot_memory_keywords (rowid, keywords) VALUES (?, ?)")
            .bind(rowid)
            .bind(keywords.index_text())
            .execute(&mut *tx)
            .await?;
        let memory = sqlx::query_as("SELECT * FROM bot_memories WHERE rowid = ?")
            .bind(rowid)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(memory)
    }

    pub async fn update(
        &self,
        input: UpdateMemory,
        keywords: Keywords,
        workspace: Option<&str>,
        now: &str,
    ) -> Result<()> {
        let mut tx = self.0.begin().await?;
        // Feedback owns last_read_at; never write a previously read copy of that field.
        let rowid: i64 = sqlx::query_scalar(
            "UPDATE bot_memories SET kind = ?, content = ?, keywords = ?, importance = ?, occurred_at = ?, sources = ?, updated_at = ?
             WHERE id = ? AND (? IS NULL OR workspace_id = ?) RETURNING rowid"
        )
        .bind(input.kind).bind(input.content).bind(sqlx::types::Json(&keywords.words))
        .bind(input.importance).bind(input.occurred_at).bind(input.sources).bind(now)
        .bind(input.id).bind(workspace).bind(workspace)
        .fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        sqlx::query("DELETE FROM bot_memory_keywords WHERE rowid = ?")
            .bind(rowid)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO bot_memory_keywords (rowid, keywords) VALUES (?, ?)")
            .bind(rowid)
            .bind(keywords.index_text())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn archive(
        &self,
        input: ArchiveMemory,
        workspace: Option<&str>,
        now: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE bot_memories SET archived = ?, updated_at = ? WHERE id = ? AND (? IS NULL OR workspace_id = ?)"
        ).bind(input.archived).bind(now).bind(input.id).bind(workspace).bind(workspace)
            .execute(&self.0).await?;
        if result.rows_affected() == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub async fn edge(
        &self,
        a: &str,
        b: &str,
        link: bool,
        workspace: Option<&str>,
        now: &str,
    ) -> Result<u64> {
        let mut tx = self.0.begin_with("BEGIN IMMEDIATE").await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM bot_memories WHERE id IN (?, ?) AND archived = 0 AND (? IS NULL OR workspace_id = ?)"
        ).bind(a).bind(b).bind(workspace).bind(workspace).fetch_one(&mut *tx).await?;
        if count != 2 {
            return Err(Error::NotFound);
        }
        let result = if link {
            sqlx::query("INSERT INTO bot_memory_edges (memory_a_id, memory_b_id, created_at) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
                .bind(a).bind(b).bind(now).execute(&mut *tx).await?
        } else {
            sqlx::query("DELETE FROM bot_memory_edges WHERE memory_a_id = ? AND memory_b_id = ?")
                .bind(a)
                .bind(b)
                .execute(&mut *tx)
                .await?
        };
        tx.commit().await?;
        Ok(result.rows_affected())
    }

    pub async fn read(&self, ids: &[String], workspace: Option<&str>) -> Result<Vec<Memory>> {
        let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM bot_memories WHERE ");
        append_ids(&mut query, ids, workspace);
        query.push(" ORDER BY id");
        Ok(query.build_query_as().fetch_all(&self.0).await?)
    }

    pub async fn feedback(
        &self,
        ids: &[String],
        workspace: Option<&str>,
        now: &str,
    ) -> Result<u64> {
        let mut query = QueryBuilder::<Sqlite>::new("UPDATE bot_memories SET last_read_at = ");
        query.push_bind(now).push(" WHERE ");
        append_ids(&mut query, ids, workspace);
        Ok(query.build().execute(&self.0).await?.rows_affected())
    }

    pub async fn candidates(
        &self,
        keywords: &Keywords,
        workspace: Option<&str>,
    ) -> Result<Vec<(Memory, f64)>> {
        let rows = sqlx::query(
            "SELECT m.*, -bm25(bot_memory_keywords) AS relevance
             FROM bot_memory_keywords JOIN bot_memories m ON m.rowid = bot_memory_keywords.rowid
             WHERE bot_memory_keywords MATCH ? AND m.archived = 0 AND (? IS NULL OR m.workspace_id = ?)
             ORDER BY bm25(bot_memory_keywords), m.id LIMIT 100"
        ).bind(keywords.query()).bind(workspace).bind(workspace).fetch_all(&self.0).await?;
        rows.into_iter()
            .map(|row| Ok((sqlx::FromRow::from_row(&row)?, row.try_get("relevance")?)))
            .collect()
    }

    pub async fn neighbors(
        &self,
        id: &str,
        workspace: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        Ok(sqlx::query_as(
            "SELECT m.* FROM bot_memories origin
             JOIN bot_memory_edges e ON e.memory_a_id = origin.id OR e.memory_b_id = origin.id
             JOIN bot_memories m ON m.id = CASE WHEN e.memory_a_id = origin.id THEN e.memory_b_id ELSE e.memory_a_id END
             WHERE origin.id = ? AND origin.archived = 0 AND m.archived = 0
               AND (? IS NULL OR (origin.workspace_id = ? AND m.workspace_id = ?))
               AND (? IS NULL OR m.id > ?)
             ORDER BY m.id LIMIT ?"
        ).bind(id).bind(workspace).bind(workspace).bind(workspace).bind(after).bind(after)
            .bind((limit + 1) as i64).fetch_all(&self.0).await?)
    }
}

fn append_ids<'a>(
    query: &mut QueryBuilder<'a, Sqlite>,
    ids: &'a [String],
    workspace: Option<&'a str>,
) {
    query
        .push("archived = 0 AND (")
        .push_bind(workspace)
        .push(" IS NULL OR workspace_id = ")
        .push_bind(workspace)
        .push(") AND id IN (");
    let mut separated = query.separated(", ");
    for id in ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");
}
