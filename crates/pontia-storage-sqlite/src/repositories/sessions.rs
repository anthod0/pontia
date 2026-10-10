use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::models::sessions::{SessionProjectionRow, SessionRow};

use pontia_core::Result;

const LOAD_SESSION_PROJECTIONS_SQL: &str = "SELECT session_id, client_type, title, handle, role, description, execution_profile_id, execution_profile_version, state, current_turn_id, state_version, metadata FROM sessions WHERE session_id = ?";

#[derive(Debug, Clone)]
pub struct SessionProjectionUpsertRecord {
    pub session_id: String,
    pub client_type: String,
    pub title: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    pub state: String,
    pub current_turn_id: Option<String>,
    pub state_version: i64,
    pub metadata: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StartingSessionRow {
    pub startup_event_id: String,
    pub session_id: String,
    pub client_type: String,
    pub runtime_id: Option<String>,
    pub tmux_socket_path: Option<String>,
    pub tmux_pane_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SessionListOptions {
    pub include_archived: bool,
    pub limit: Option<u32>,
    pub include_pinned: bool,
}

#[derive(Debug, Clone)]
pub struct SessionOverviewOptions {
    pub include_pinned: bool,
    pub include_archived: bool,
    pub include_active: bool,
    pub include_list: bool,
    pub workspace_id: Option<String>,
    pub cursor: Option<SessionOverviewCursor>,
    pub list_limit: u32,
}

#[derive(Debug, Clone)]
pub struct SessionOverviewCursor {
    pub updated_at: String,
    pub session_id: String,
}

#[derive(Debug)]
pub struct SessionOverviewRows {
    pub pinned: Option<Vec<SessionRow>>,
    pub archived: Option<Vec<SessionRow>>,
    pub active: Option<Vec<SessionRow>>,
    pub list: Option<SessionOverviewListRows>,
    pub workspace_exists: bool,
}

#[derive(Debug)]
pub struct SessionOverviewListRows {
    pub sessions: Vec<SessionRow>,
    pub next_cursor: Option<SessionOverviewCursor>,
}

#[derive(Debug, Clone)]
pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn load_projection_rows(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionProjectionRow>> {
        Ok(
            sqlx::query_as::<_, SessionProjectionRow>(LOAD_SESSION_PROJECTIONS_SQL)
                .bind(session_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn load_projection_rows_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<Vec<SessionProjectionRow>> {
        Ok(
            sqlx::query_as::<_, SessionProjectionRow>(LOAD_SESSION_PROJECTIONS_SQL)
                .bind(session_id)
                .fetch_all(&mut **tx)
                .await?,
        )
    }

    pub async fn session_exists_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id = ?)",
        )
        .bind(session_id)
        .fetch_one(&mut **tx)
        .await?
            != 0)
    }

    pub async fn upsert_projection_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session: SessionProjectionUpsertRecord,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO sessions
               (session_id, client_type, title, handle, role, description, execution_profile_id,
                execution_profile_version, state, current_turn_id, state_version, metadata)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   client_type = excluded.client_type,
                   title = excluded.title,
                   handle = excluded.handle,
                   role = excluded.role,
                   description = excluded.description,
                   execution_profile_id = excluded.execution_profile_id,
                   execution_profile_version = excluded.execution_profile_version,
                   state = excluded.state,
                   current_turn_id = excluded.current_turn_id,
                   state_version = excluded.state_version,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(session.session_id)
        .bind(session.client_type)
        .bind(session.title)
        .bind(session.handle)
        .bind(session.role)
        .bind(session.description)
        .bind(session.execution_profile_id)
        .bind(session.execution_profile_version)
        .bind(session.state)
        .bind(session.current_turn_id)
        .bind(session.state_version)
        .bind(session.metadata)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionRow>> {
        self.list_sessions_with_options(SessionListOptions::default())
            .await
    }

    pub async fn list_sessions_with_options(
        &self,
        options: SessionListOptions,
    ) -> Result<Vec<SessionRow>> {
        let limit = options.limit.map(i64::from);
        Ok(sqlx::query_as::<_, SessionRow>(
            r#"WITH ordered_sessions AS (
                   SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                          s.execution_profile_id, s.execution_profile_version,
                          s.state, s.current_turn_id, s.workspace_id,
                          COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                          s.pinned_at, s.archived_at,
                          s.metadata, s.created_at, s.updated_at,
                          ROW_NUMBER() OVER (
                              ORDER BY s.pinned_at IS NULL, s.pinned_at DESC, s.updated_at DESC, s.session_id
                          ) AS row_num,
                          ROW_NUMBER() OVER (
                              PARTITION BY s.pinned_at IS NOT NULL
                              ORDER BY s.updated_at DESC, s.session_id
                          ) AS unpinned_row_num
                   FROM sessions s
                   JOIN workspaces w ON w.workspace_id = s.workspace_id AND w.state = 'active'
                   WHERE (? OR s.archived_at IS NULL)
               )
               SELECT session_id, client_type, title, handle, role, description,
                      execution_profile_id, execution_profile_version,
                      state, current_turn_id, workspace_id, workspace_ref,
                      pinned_at, archived_at,
                      metadata, created_at, updated_at
               FROM ordered_sessions
               WHERE (? IS NULL
                      OR (? AND (pinned_at IS NOT NULL OR (pinned_at IS NULL AND unpinned_row_num <= ?)))
                      OR (NOT ? AND row_num <= ?))
               ORDER BY pinned_at IS NULL, pinned_at DESC, updated_at DESC, session_id"#,
        )
        .bind(options.include_archived)
        .bind(limit)
        .bind(options.include_pinned)
        .bind(limit)
        .bind(options.include_pinned)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn session_overview(
        &self,
        options: SessionOverviewOptions,
    ) -> Result<SessionOverviewRows> {
        let mut tx = self.pool.begin().await?;
        let workspace_exists = match options.workspace_id.as_deref() {
            Some(workspace_id) => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM workspaces WHERE workspace_id = ?)",
                )
                .bind(workspace_id)
                .fetch_one(&mut *tx)
                .await?
                    != 0
            }
            None => true,
        };

        let pinned = if options.include_pinned {
            Some(
                sqlx::query_as::<_, SessionRow>(
                    r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                              s.execution_profile_id, s.execution_profile_version,
                              s.state, s.current_turn_id, s.workspace_id,
                              COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                              s.pinned_at, s.archived_at, s.metadata, s.created_at, s.updated_at
                       FROM sessions s
                       LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
                       WHERE s.archived_at IS NULL AND s.pinned_at IS NOT NULL
                       ORDER BY s.pinned_at DESC, s.session_id DESC"#,
                )
                .fetch_all(&mut *tx)
                .await?,
            )
        } else {
            None
        };

        let archived = if options.include_archived {
            Some(
                sqlx::query_as::<_, SessionRow>(
                    r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                              s.execution_profile_id, s.execution_profile_version,
                              s.state, s.current_turn_id, s.workspace_id,
                              COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                              s.pinned_at, s.archived_at, s.metadata, s.created_at, s.updated_at
                       FROM sessions s
                       LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
                       WHERE s.archived_at IS NOT NULL
                       ORDER BY s.archived_at DESC, s.session_id DESC"#,
                )
                .fetch_all(&mut *tx)
                .await?,
            )
        } else {
            None
        };

        let active = if options.include_active {
            Some(
                sqlx::query_as::<_, SessionRow>(
                    r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                              s.execution_profile_id, s.execution_profile_version,
                              s.state, s.current_turn_id, s.workspace_id,
                              COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                              s.pinned_at, s.archived_at, s.metadata, s.created_at, s.updated_at
                       FROM sessions s INDEXED BY idx_sessions_active_updated
                       LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
                       WHERE s.archived_at IS NULL AND s.state NOT IN ('exited', 'error')
                       ORDER BY s.updated_at DESC, s.session_id DESC"#,
                )
                .fetch_all(&mut *tx)
                .await?,
            )
        } else {
            None
        };

        let list = if options.include_list && workspace_exists {
            let fetch_limit = i64::from(options.list_limit) + 1;
            let mut sessions = match (options.workspace_id.as_deref(), options.cursor.as_ref()) {
                (None, None) => sqlx::query_as::<_, SessionRow>(
                    r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                                  s.execution_profile_id, s.execution_profile_version,
                                  s.state, s.current_turn_id, s.workspace_id,
                                  COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                                  s.pinned_at, s.archived_at, s.metadata, s.created_at, s.updated_at
                           FROM sessions s INDEXED BY idx_sessions_unarchived_updated
                           LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
                           WHERE s.archived_at IS NULL
                           ORDER BY s.updated_at DESC, s.session_id DESC
                           LIMIT ?"#,
                )
                .bind(fetch_limit)
                .fetch_all(&mut *tx)
                .await?,
                (None, Some(cursor)) => sqlx::query_as::<_, SessionRow>(
                    r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                                  s.execution_profile_id, s.execution_profile_version,
                                  s.state, s.current_turn_id, s.workspace_id,
                                  COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                                  s.pinned_at, s.archived_at, s.metadata, s.created_at, s.updated_at
                           FROM sessions s INDEXED BY idx_sessions_unarchived_updated
                           LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
                           WHERE s.archived_at IS NULL
                             AND (s.updated_at < ? OR (s.updated_at = ? AND s.session_id < ?))
                           ORDER BY s.updated_at DESC, s.session_id DESC
                           LIMIT ?"#,
                )
                .bind(&cursor.updated_at)
                .bind(&cursor.updated_at)
                .bind(&cursor.session_id)
                .bind(fetch_limit)
                .fetch_all(&mut *tx)
                .await?,
                (Some(workspace_id), None) => sqlx::query_as::<_, SessionRow>(
                    r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                                  s.execution_profile_id, s.execution_profile_version,
                                  s.state, s.current_turn_id, s.workspace_id,
                                  COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                                  s.pinned_at, s.archived_at, s.metadata, s.created_at, s.updated_at
                           FROM sessions s INDEXED BY idx_sessions_workspace_unarchived_updated
                           LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
                           WHERE s.workspace_id = ? AND s.archived_at IS NULL
                           ORDER BY s.updated_at DESC, s.session_id DESC
                           LIMIT ?"#,
                )
                .bind(workspace_id)
                .bind(fetch_limit)
                .fetch_all(&mut *tx)
                .await?,
                (Some(workspace_id), Some(cursor)) => sqlx::query_as::<_, SessionRow>(
                    r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                                  s.execution_profile_id, s.execution_profile_version,
                                  s.state, s.current_turn_id, s.workspace_id,
                                  COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                                  s.pinned_at, s.archived_at, s.metadata, s.created_at, s.updated_at
                           FROM sessions s INDEXED BY idx_sessions_workspace_unarchived_updated
                           LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
                           WHERE s.workspace_id = ? AND s.archived_at IS NULL
                             AND (s.updated_at < ? OR (s.updated_at = ? AND s.session_id < ?))
                           ORDER BY s.updated_at DESC, s.session_id DESC
                           LIMIT ?"#,
                )
                .bind(workspace_id)
                .bind(&cursor.updated_at)
                .bind(&cursor.updated_at)
                .bind(&cursor.session_id)
                .bind(fetch_limit)
                .fetch_all(&mut *tx)
                .await?,
            };
            let has_next_page = sessions.len() > options.list_limit as usize;
            sessions.truncate(options.list_limit as usize);
            let next_cursor = has_next_page.then(|| {
                let last = sessions
                    .last()
                    .expect("a next page requires a returned row");
                SessionOverviewCursor {
                    updated_at: last.updated_at.clone(),
                    session_id: last.session_id.clone(),
                }
            });
            Some(SessionOverviewListRows {
                sessions,
                next_cursor,
            })
        } else if options.include_list {
            Some(SessionOverviewListRows {
                sessions: Vec::new(),
                next_cursor: None,
            })
        } else {
            None
        };

        tx.commit().await?;
        Ok(SessionOverviewRows {
            pinned,
            archived,
            active,
            list,
            workspace_exists,
        })
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionRow>> {
        Ok(sqlx::query_as::<_, SessionRow>(
            r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                      s.execution_profile_id, s.execution_profile_version,
                      s.state, s.current_turn_id, s.workspace_id,
                      COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                      s.pinned_at, s.archived_at,
                      s.metadata, s.created_at, s.updated_at
               FROM sessions s
               LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
               WHERE s.session_id = ?"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn starting_sessions_before(&self, cutoff: &str) -> Result<Vec<StartingSessionRow>> {
        Ok(sqlx::query_as::<_, StartingSessionRow>(
            r#"SELECT s.session_id, s.client_type,
                      r.runtime_id, r.tmux_socket_path, r.tmux_pane_id,
                      (SELECT e.event_id FROM events e WHERE e.session_id=s.session_id
                       AND e.event_type IN ('session.starting','session.resuming')
                       ORDER BY e.rowid DESC LIMIT 1) AS startup_event_id
               FROM sessions s
               LEFT JOIN session_runtimes r ON r.session_id = s.session_id AND r.role = 'tui'
               WHERE s.state = 'starting'
                 AND (
                     SELECT e.created_at
                     FROM events e
                     WHERE e.session_id = s.session_id
                       AND e.event_type IN ('session.starting', 'session.resuming')
                     ORDER BY e.rowid DESC
                     LIMIT 1
                 ) <= ?
               ORDER BY s.session_id"#,
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn exists(&self, session_id: &str) -> Result<bool> {
        let exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id = ?)")
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(exists != 0)
    }

    pub async fn active_session_id_for_handle(
        &self,
        workspace_id: &str,
        handle: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT session_id FROM sessions WHERE workspace_id = ? AND handle = ? AND state NOT IN ('exited', 'error') LIMIT 1",
        )
        .bind(workspace_id)
        .bind(handle)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn update_session_workspace(
        &self,
        session_id: &str,
        workspace_ref: Option<&str>,
        workspace_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE sessions SET workspace_ref = ?, workspace_id = ? WHERE session_id = ?")
            .bind(workspace_ref)
            .bind(workspace_id)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn pin_session(&self, session_id: &str) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE sessions SET pinned_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE session_id = ?",
        )
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn unpin_session(&self, session_id: &str) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE sessions SET pinned_at = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE session_id = ?",
        )
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn archive_session(&self, session_id: &str) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE sessions SET archived_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), pinned_at = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE session_id = ?",
        )
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn unarchive_session(&self, session_id: &str) -> Result<u64> {
        let result = sqlx::query(
            "UPDATE sessions SET archived_at = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE session_id = ?",
        )
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
