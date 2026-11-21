use pontia_core::Result;
pub use pontia_storage_sqlite::models::session_files::SharedFile;
use sqlx::{QueryBuilder, Sqlite};

use super::{ControlQueryService, TimePage};

/// One `pontia send-file` call from a Session owned by an application.
pub struct OwnedFileShare {
    pub share_id: String,
    pub session_id: String,
    pub created_at: String,
    pub timeline_ms: i64,
    /// In share order.
    pub files: Vec<SharedFile>,
}

#[derive(sqlx::FromRow)]
struct ShareRow {
    share_id: String,
    session_id: String,
    created_at: String,
    timeline_ms: i64,
}

fn owned_shares<'a>(
    application_id: &'a str,
    reference_prefix: &'a str,
) -> QueryBuilder<'a, Sqlite> {
    // Shares are timestamped in UTC. Truncate before SQLite rounds fractional seconds.
    // Files of one share have the same time, so grouping keeps it exact.
    let mut query = QueryBuilder::new(
        "SELECT * FROM (SELECT f.share_id, f.session_id, f.created_at,
             CAST(round(unixepoch(substr(f.created_at, 1, 23), 'subsec') * 1000) AS INTEGER) AS timeline_ms
         FROM sessions s JOIN session_files f USING (session_id) WHERE s.application_id = ",
    );
    query
        .push_bind(application_id)
        .push(" AND substr(s.application_ref, 1, length(")
        .push_bind(reference_prefix)
        .push(")) = ")
        .push_bind(reference_prefix)
        .push(" GROUP BY f.share_id) WHERE 1");
    query
}

impl ControlQueryService {
    pub async fn owned_file_shares(
        &self,
        application_id: &str,
        reference_prefix: &str,
        page: TimePage<'_>,
        limit: u32,
    ) -> Result<Vec<OwnedFileShare>> {
        let mut query = owned_shares(application_id, reference_prefix);
        if let TimePage::Before(ms, id) | TimePage::After(ms, id) = page {
            query.push(if matches!(page, TimePage::After(..)) {
                " AND (timeline_ms, share_id) > ("
            } else {
                " AND (timeline_ms, share_id) < ("
            });
            query.push_bind(ms).push(", ").push_bind(id).push(")");
        }
        query
            .push(if matches!(page, TimePage::After(..)) {
                " ORDER BY timeline_ms, share_id LIMIT "
            } else {
                " ORDER BY timeline_ms DESC, share_id DESC LIMIT "
            })
            .push_bind(limit);
        let rows: Vec<ShareRow> = query.build_query_as().fetch_all(&self.pool).await?;
        self.with_files(rows).await
    }

    /// The share with this share ID, or the share containing this file ID.
    pub async fn owned_file_share(
        &self,
        application_id: &str,
        reference_prefix: &str,
        id: &str,
    ) -> Result<Option<OwnedFileShare>> {
        let mut query = owned_shares(application_id, reference_prefix);
        query
            .push(" AND share_id = (SELECT share_id FROM session_files WHERE share_id = ")
            .push_bind(id)
            .push(" OR file_id = ")
            .push_bind(id)
            .push(" LIMIT 1)");
        let rows: Vec<ShareRow> = query.build_query_as().fetch_all(&self.pool).await?;
        Ok(self.with_files(rows).await?.pop())
    }

    async fn with_files(&self, rows: Vec<ShareRow>) -> Result<Vec<OwnedFileShare>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT file_id, share_id, turn_id, path, name, size, media_type, created_at FROM session_files WHERE share_id IN (",
        );
        let mut ids = query.separated(", ");
        for row in &rows {
            ids.push_bind(&row.share_id);
        }
        query.push(") ORDER BY file_id");
        let mut files: Vec<SharedFile> = query.build_query_as().fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let (mine, rest) = files.drain(..).partition(|f| f.share_id == row.share_id);
                files = rest;
                OwnedFileShare {
                    share_id: row.share_id,
                    session_id: row.session_id,
                    created_at: row.created_at,
                    timeline_ms: row.timeline_ms,
                    files: mine,
                }
            })
            .collect())
    }
}
