mod render;

use pontia_agent_control::{ControlQueryService, LaunchableAgentProfile};
use pontia_core::domain::SessionState;
use sqlx::SqlitePool;

use crate::{
    identity::BotRole,
    memory::Memory,
    messages::{BotMessage, MESSAGE_COLUMNS},
    user::UserService,
};

pub use render::estimate_tokens;

#[derive(sqlx::FromRow)]
struct HistoryMessage {
    #[sqlx(flatten)]
    message: BotMessage,
    #[sqlx(json)]
    related_to: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct OpenTodo {
    id: String,
    created_at: String,
    summary: String,
    total: i64,
}

/// Trusted Bot associations and Agent Control state, supplied by helper orchestration.
#[derive(Clone, Debug)]
pub struct HelperContext {
    pub bot_id: String,
    pub session_id: String,
    pub role: BotRole,
    pub workspace_id: Option<String>,
    pub description: Option<String>,
    pub state: SessionState,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    User(#[from] crate::user::Error),
    #[error(transparent)]
    Control(#[from] pontia_core::Error),
    #[error(transparent)]
    Storage(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct HandoffService {
    pool: SqlitePool,
    user: UserService,
    queries: ControlQueryService,
    roots: Vec<pontia_config::WorkspaceRootConfig>,
}

impl HandoffService {
    pub fn new(
        pool: SqlitePool,
        user: UserService,
        queries: ControlQueryService,
        roots: Vec<pontia_config::WorkspaceRootConfig>,
    ) -> Self {
        Self {
            pool,
            user,
            queries,
            roots,
        }
    }

    /// Reads historical context only. Deliver the current input separately, including retries.
    /// The caller supplies helper associations and launchable helper Profiles; native
    /// conversations are never queried.
    pub async fn generate(
        &self,
        bot_id: &str,
        helpers: &[HelperContext],
        profiles: &[LaunchableAgentProfile],
        default_workspace_id: &str,
    ) -> Result<String, Error> {
        self.generate_for_input(bot_id, helpers, profiles, &[], default_workspace_id)
            .await
    }

    pub(crate) async fn generate_for_input(
        &self,
        bot_id: &str,
        helpers: &[HelperContext],
        profiles: &[LaunchableAgentProfile],
        pending_message_ids: &[String],
        default_workspace_id: &str,
    ) -> Result<String, Error> {
        let description: Option<String> =
            sqlx::query_scalar("SELECT description FROM bots WHERE bot_id = ?")
                .bind(bot_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| pontia_core::Error::NotFound(format!("Bot {bot_id} not found")))?;
        let user = self.user.read_for_handoff().await?;
        let mut workspaces = self.queries.list_workspaces().await?;
        workspaces.sort_by(|a, b| a.workspace_id.cmp(&b.workspace_id));
        // Memory timestamps are UTC RFC 3339 with optional fractional seconds.
        // Pad that fraction for ordering without SQLite's millisecond rounding.
        let memories: Vec<Memory> = sqlx::query_as(
            "SELECT * FROM bot_memories WHERE archived = 0
             ORDER BY substr(created_at, 1, 19) DESC,
                 substr(rtrim(substr(created_at, 21), 'Z') || '000000000', 1, 9) DESC,
                 id DESC LIMIT 30",
        )
        .fetch_all(&self.pool)
        .await?;
        let messages: Vec<HistoryMessage> = sqlx::query_as(
            &format!("SELECT {MESSAGE_COLUMNS}, (SELECT json_group_array(target_id ORDER BY target_id) FROM bot_message_links
                            WHERE message_id = m.id AND (m.kind = 'bot_reply' OR m.link_source = 'bot')) AS related_to
             FROM bot_messages m WHERE bot_id = ?
             AND id NOT IN (SELECT value FROM json_each(?))
             ORDER BY created_at DESC, id DESC LIMIT 60"),
        )
        .bind(bot_id)
        .bind(serde_json::json!(pending_message_ids).to_string())
        .fetch_all(&self.pool)
        .await?;
        let todos: Vec<OpenTodo> = sqlx::query_as(
            "SELECT m.id, m.created_at, substr(m.content, 1, 160) AS summary, count(*) OVER () AS total
             FROM bot_messages m JOIN bot_todos t ON t.message_id = m.id
             WHERE m.bot_id = ? AND t.state = 'open' ORDER BY timeline_ms DESC, m.id DESC LIMIT 20",
        ).bind(bot_id).fetch_all(&self.pool).await?;
        let mut helpers: Vec<_> = helpers
            .iter()
            .filter(|helper| {
                helper.bot_id == bot_id
                    && matches!(helper.role, BotRole::Helper | BotRole::Retrieval)
                    && helper.state != SessionState::Exited
            })
            .collect();
        helpers.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Ok(render::document(
            &user,
            description.as_deref(),
            &workspaces,
            default_workspace_id,
            &self.roots,
            &memories,
            profiles,
            &helpers,
            &messages,
            &todos,
        ))
    }
}
