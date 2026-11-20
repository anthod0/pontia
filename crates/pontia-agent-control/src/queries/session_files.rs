use pontia_core::Result;
pub use pontia_storage_sqlite::models::session_files::SharedFile;
use sqlx::{QueryBuilder, Sqlite};

use super::{ControlQueryService, TimePage};

#[derive(sqlx::FromRow)]
pub struct OwnedSessionFile {
    pub session_id: String,
    pub timeline_ms: i64,
    #[sqlx(flatten)]
    pub file: SharedFile,
}

fn owned_files<'a>(application_id: &'a str, reference_prefix: &'a str) -> QueryBuilder<'a, Sqlite> {
    // Shares are timestamped in UTC. Truncate before SQLite rounds fractional seconds.
    let mut query = QueryBuilder::new(
        "SELECT f.*, CAST(round(unixepoch(substr(f.created_at, 1, 23), 'subsec') * 1000) AS INTEGER) AS timeline_ms
         FROM sessions s JOIN session_files f USING (session_id) WHERE s.application_id = ",
    );
    query
        .push_bind(application_id)
        .push(" AND substr(s.application_ref, 1, length(")
        .push_bind(reference_prefix)
        .push(")) = ")
        .push_bind(reference_prefix);
    query
}

impl ControlQueryService {
    pub async fn owned_session_files(
        &self,
        application_id: &str,
        reference_prefix: &str,
        page: TimePage<'_>,
        limit: u32,
    ) -> Result<Vec<OwnedSessionFile>> {
        let mut query = owned_files(application_id, reference_prefix);
        if let TimePage::Before(ms, id) | TimePage::After(ms, id) = page {
            query.push(if matches!(page, TimePage::After(..)) {
                " AND (timeline_ms, f.file_id) > ("
            } else {
                " AND (timeline_ms, f.file_id) < ("
            });
            query.push_bind(ms).push(", ").push_bind(id).push(")");
        }
        query
            .push(if matches!(page, TimePage::After(..)) {
                " ORDER BY timeline_ms, f.file_id LIMIT "
            } else {
                " ORDER BY timeline_ms DESC, f.file_id DESC LIMIT "
            })
            .push_bind(limit);
        Ok(query.build_query_as().fetch_all(&self.pool).await?)
    }

    pub async fn owned_session_file(
        &self,
        application_id: &str,
        reference_prefix: &str,
        file_id: &str,
    ) -> Result<Option<OwnedSessionFile>> {
        let mut query = owned_files(application_id, reference_prefix);
        query.push(" AND f.file_id = ").push_bind(file_id);
        Ok(query.build_query_as().fetch_optional(&self.pool).await?)
    }
}
