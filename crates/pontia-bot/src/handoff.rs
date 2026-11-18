mod render;

use pontia_agent_control::ControlQueryService;
use pontia_core::domain::SessionState;
use sqlx::SqlitePool;

use crate::{identity::BotRole, memory::Memory, messages::BotMessage, user::UserService};

pub use render::estimate_tokens;

/// Trusted Bot associations and Agent Control state, supplied by helper orchestration.
#[derive(Clone, Debug)]
pub struct HelperContext {
    pub bot_id: String,
    pub session_id: String,
    pub role: BotRole,
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
}

impl HandoffService {
    pub fn new(pool: SqlitePool, user: UserService, queries: ControlQueryService) -> Self {
        Self {
            pool,
            user,
            queries,
        }
    }

    /// Reads historical context only. Deliver the current input separately, including retries.
    /// The caller supplies helper associations; native conversations are never queried.
    pub async fn generate(&self, bot_id: &str, helpers: &[HelperContext]) -> Result<String, Error> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM bots WHERE bot_id = ?)")
            .bind(bot_id)
            .fetch_one(&self.pool)
            .await?;
        if !exists {
            return Err(pontia_core::Error::NotFound(format!("Bot {bot_id} not found")).into());
        }
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
        let messages: Vec<BotMessage> = sqlx::query_as(
            "SELECT * FROM bot_messages WHERE bot_id = ?
             ORDER BY created_at DESC, id DESC LIMIT 60",
        )
        .bind(bot_id)
        .fetch_all(&self.pool)
        .await?;
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
            &workspaces,
            &memories,
            &helpers,
            &messages,
        ))
    }
}
