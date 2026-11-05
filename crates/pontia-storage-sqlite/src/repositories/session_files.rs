use super::events::{EventInsertRecord, SqliteEventRepository};
use pontia_core::{Error, Result};
use sqlx::SqlitePool;

use crate::{models::session_files::SharedFile, repositories::turns::SqliteTurnRepository};

#[derive(Clone)]
pub struct SessionFileRepository {
    pool: SqlitePool,
}

impl SessionFileRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn share(
        &self,
        session_id: &str,
        mut file: SharedFile,
        event_id: &str,
    ) -> Result<SharedFile> {
        let mut tx = self.pool.begin().await?;
        SqliteTurnRepository::serialize_session_turn_writes_in_tx(&mut tx, session_id).await?;
        let client_type: String =
            sqlx::query_scalar("SELECT client_type FROM sessions WHERE session_id = ?")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?;
        file.turn_id = sqlx::query_scalar("SELECT turn_id FROM turns WHERE session_id = ? AND state = 'running' ORDER BY created_at DESC LIMIT 1")
            .bind(session_id).fetch_optional(&mut *tx).await?;
        sqlx::query("INSERT INTO session_files (file_id, session_id, turn_id, path, name, size, media_type, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&file.file_id).bind(session_id).bind(&file.turn_id).bind(&file.path)
            .bind(&file.name).bind(file.size).bind(&file.media_type).bind(&file.created_at)
            .execute(&mut *tx).await?;
        SqliteEventRepository::insert_event_in_tx(
            &mut tx,
            EventInsertRecord {
                event_id: event_id.to_owned(),
                session_id: session_id.to_owned(),
                turn_id: file.turn_id.clone(),
                report_key: None,
                source: "external_api".into(),
                client_type,
                event_type: "session.file_shared".into(),
                occurred_at: file.created_at.clone(),
                payload: serde_json::json!({"file_id": file.file_id}).to_string(),
                timeline_boundary: None,
                turn_topology: None,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(file)
    }

    pub async fn get(&self, session_id: &str, file_id: &str) -> Result<Option<SharedFile>> {
        Ok(sqlx::query_as("SELECT file_id, turn_id, path, name, size, media_type, created_at FROM session_files WHERE session_id = ? AND file_id = ?")
            .bind(session_id).bind(file_id).fetch_optional(&self.pool).await?)
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
        Ok(sqlx::query_as("SELECT file_id, turn_id, path, name, size, media_type, created_at FROM session_files WHERE session_id = ? AND (? IS NULL OR (created_at, file_id) < (?, ?)) ORDER BY created_at DESC, file_id DESC LIMIT ?")
            .bind(session_id).bind(cursor.as_ref().map(|f| &f.file_id))
            .bind(cursor.as_ref().map(|f| &f.created_at)).bind(cursor.as_ref().map(|f| &f.file_id))
            .bind(limit).fetch_all(&self.pool).await?)
    }
}
