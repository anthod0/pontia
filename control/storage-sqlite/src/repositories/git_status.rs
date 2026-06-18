use pontia_core::Result;
use sqlx::SqlitePool;

use crate::models::git_status::WorkspaceGitStatusRow;

#[derive(Debug, Clone)]
pub struct GitStatusUpsertRecord {
    pub workspace_id: String,
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub staged_count: i64,
    pub unstaged_count: i64,
    pub untracked_count: i64,
    pub conflicted_count: i64,
    pub clean: bool,
    pub state: String,
    pub failure: Option<String>,
    pub observed_at: String,
}

#[derive(Debug, Clone)]
pub struct SqliteGitStatusRepository {
    pool: SqlitePool,
}

impl SqliteGitStatusRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn workspace_exists(&self, workspace_id: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workspaces WHERE workspace_id = ? AND state != 'deleted'",
        )
        .bind(workspace_id)
        .fetch_one(&self.pool)
        .await?
            > 0)
    }

    pub async fn get_status(&self, workspace_id: &str) -> Result<Option<WorkspaceGitStatusRow>> {
        Ok(sqlx::query_as::<_, WorkspaceGitStatusRow>(
            r#"SELECT workspace_id, repo_root, branch, upstream, ahead, behind, staged_count,
                      unstaged_count, untracked_count, conflicted_count, clean, state, failure,
                      observed_at, updated_at
               FROM workspace_git_status
               WHERE workspace_id = ?"#,
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn upsert_status(&self, record: GitStatusUpsertRecord) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO workspace_git_status
               (workspace_id, repo_root, branch, upstream, ahead, behind, staged_count, unstaged_count,
                untracked_count, conflicted_count, clean, state, failure, observed_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
               ON CONFLICT(workspace_id) DO UPDATE SET
                 repo_root = excluded.repo_root,
                 branch = excluded.branch,
                 upstream = excluded.upstream,
                 ahead = excluded.ahead,
                 behind = excluded.behind,
                 staged_count = excluded.staged_count,
                 unstaged_count = excluded.unstaged_count,
                 untracked_count = excluded.untracked_count,
                 conflicted_count = excluded.conflicted_count,
                 clean = excluded.clean,
                 state = excluded.state,
                 failure = excluded.failure,
                 observed_at = excluded.observed_at,
                 updated_at = excluded.updated_at"#,
        )
        .bind(record.workspace_id)
        .bind(record.repo_root)
        .bind(record.branch)
        .bind(record.upstream)
        .bind(record.ahead)
        .bind(record.behind)
        .bind(record.staged_count)
        .bind(record.unstaged_count)
        .bind(record.untracked_count)
        .bind(record.conflicted_count)
        .bind(record.clean)
        .bind(record.state)
        .bind(record.failure)
        .bind(record.observed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn current_timestamp(&self) -> Result<String> {
        Ok(
            sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')")
                .fetch_one(&self.pool)
                .await?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect_sqlite, run_migrations};

    async fn pool() -> sqlx::SqlitePool {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("git-status.db");
        let _kept_dir = dir.keep();
        let database_url = format!("sqlite://{}", db_path.display());
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        db
    }

    #[tokio::test]
    async fn upserts_and_reads_workspace_git_status() {
        let pool = pool().await;
        sqlx::query("INSERT INTO workspaces (workspace_id, canonical_path, display_path, name) VALUES ('ws_1', '/tmp/ws_1', '/tmp/ws_1', 'ws_1')")
            .execute(&pool)
            .await
            .unwrap();
        let repo = SqliteGitStatusRepository::new(pool);

        repo.upsert_status(GitStatusUpsertRecord {
            workspace_id: "ws_1".to_string(),
            repo_root: Some("/tmp/ws_1".to_string()),
            branch: Some("main".to_string()),
            upstream: None,
            ahead: 1,
            behind: 0,
            staged_count: 2,
            unstaged_count: 3,
            untracked_count: 4,
            conflicted_count: 0,
            clean: false,
            state: "observed".to_string(),
            failure: None,
            observed_at: "2026-01-01T00:00:00.000Z".to_string(),
        })
        .await
        .unwrap();

        let row = repo.get_status("ws_1").await.unwrap().unwrap();
        assert_eq!(row.branch.as_deref(), Some("main"));
        assert_eq!(row.staged_count, 2);
    }
}
