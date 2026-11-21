use super::events::{EventInsertRecord, SqliteEventRepository};
use pontia_core::{Error, Result};
use sqlx::SqlitePool;

use crate::{models::session_files::SharedFile, repositories::turns::SqliteTurnRepository};

const COLUMNS: &str = "file_id, share_id, turn_id, path, name, size, media_type, created_at";

#[derive(Clone)]
pub struct SessionFileRepository {
    pool: SqlitePool,
}

impl SessionFileRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Records one share of `files`, which is non-empty and all carry the same share ID and time,
    /// with its event.
    pub async fn share(
        &self,
        session_id: &str,
        mut files: Vec<SharedFile>,
        event_id: &str,
    ) -> Result<Vec<SharedFile>> {
        let (share_id, created_at) = (files[0].share_id.clone(), files[0].created_at.clone());
        let mut tx = self.pool.begin().await?;
        SqliteTurnRepository::serialize_session_turn_writes_in_tx(&mut tx, session_id).await?;
        let client_type: String =
            sqlx::query_scalar("SELECT client_type FROM sessions WHERE session_id = ?")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?;
        let turn_id: Option<String> = sqlx::query_scalar("SELECT turn_id FROM turns WHERE session_id = ? AND state = 'running' ORDER BY created_at DESC LIMIT 1")
            .bind(session_id).fetch_optional(&mut *tx).await?;
        for file in &mut files {
            file.turn_id = turn_id.clone();
            sqlx::query(&format!("INSERT INTO session_files (session_id, {COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"))
                .bind(session_id).bind(&file.file_id).bind(&file.share_id).bind(&file.turn_id)
                .bind(&file.path).bind(&file.name).bind(file.size).bind(&file.media_type)
                .bind(&file.created_at)
                .execute(&mut *tx).await?;
        }
        let file_ids: Vec<_> = files.iter().map(|file| &file.file_id).collect();
        SqliteEventRepository::insert_event_in_tx(
            &mut tx,
            EventInsertRecord {
                event_id: event_id.to_owned(),
                session_id: session_id.to_owned(),
                turn_id,
                report_key: None,
                source: "external_api".into(),
                client_type,
                event_type: "session.files_shared".into(),
                occurred_at: created_at,
                payload: serde_json::json!({"share_id": share_id, "file_ids": file_ids})
                    .to_string(),
                timeline_boundary: None,
                turn_topology: None,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(files)
    }

    pub async fn get(&self, session_id: &str, file_id: &str) -> Result<Option<SharedFile>> {
        Ok(sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM session_files WHERE session_id = ? AND file_id = ?"
        ))
        .bind(session_id)
        .bind(file_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Files of one share in the order they were shared.
    pub async fn share_files(&self, session_id: &str, share_id: &str) -> Result<Vec<SharedFile>> {
        Ok(sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM session_files WHERE session_id = ? AND share_id = ? ORDER BY file_id"
        ))
        .bind(session_id)
        .bind(share_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list(
        &self,
        session_id: &str,
        limit: i64,
        cursor: Option<&str>,
    ) -> Result<Vec<SharedFile>> {
        let cursor = match cursor {
            Some(id) => Some(
                self.get(session_id, id)
                    .await?
                    .ok_or_else(|| Error::Domain("Invalid file cursor".into()))?,
            ),
            None => None,
        };
        Ok(sqlx::query_as(&format!("SELECT {COLUMNS} FROM session_files WHERE session_id = ? AND (? IS NULL OR (created_at, file_id) < (?, ?)) ORDER BY created_at DESC, file_id DESC LIMIT ?"))
            .bind(session_id).bind(cursor.as_ref().map(|f| &f.file_id))
            .bind(cursor.as_ref().map(|f| &f.created_at)).bind(cursor.as_ref().map(|f| &f.file_id))
            .bind(limit).fetch_all(&self.pool).await?)
    }
}
