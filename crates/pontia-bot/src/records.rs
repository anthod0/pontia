use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use pontia_agent_control::{agent_clients::AgentClientService, clients::ClientRegistry};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::identity::IdentityError;
use crate::messages::{MessageKind, append_message};

mod initialization;

const BOT_DESCRIPTION_LIMIT: usize = 2_000;

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub enum Locale {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "ja")]
    Ja,
}

impl Locale {
    fn greeting(self, name: &str) -> String {
        match self {
            Self::En => format!("Hi, I'm {name}. What would you like me to help with?"),
            Self::Ja => format!("こんにちは、{name}です。何をお手伝いしましょうか？"),
            Self::ZhCn => format!("你好，我是 {name}。你想让我帮你做什么？"),
        }
    }
}

fn normalize_description(description: Option<&str>) -> Option<&str> {
    description.map(str::trim).filter(|text| !text.is_empty())
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, sqlx::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum AvatarShape {
    #[default]
    Classic,
    Mochi,
    Cloud,
    Drop,
    Blob,
    Ghost,
    Onigiri,
    Egg,
    Pudding,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, sqlx::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum AvatarColor {
    #[default]
    Blue,
    Aqua,
    Green,
    Yellow,
    Orange,
    Purple,
    Indigo,
    Brown,
    Rose,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Bot {
    pub bot_id: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar_shape: AvatarShape,
    pub avatar_color: AvatarColor,
    pub client_type: String,
    pub client_key: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub current_session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip)]
    pub(crate) awaiting_reply: bool,
    #[serde(skip)]
    pub(crate) awaiting_reply_after: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BotIdleTimer {
    pub since: String,
    pub turn_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotConfiguration {
    pub name: String,
    pub description: Option<String>,
    pub avatar_shape: Option<AvatarShape>,
    pub avatar_color: Option<AvatarColor>,
    pub client_type: String,
    pub client_key: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error(transparent)]
    Rejected(#[from] Error),
    #[error(
        "Bot {bot_id} was saved but initialization failed: {source}; retry initialize with this bot_id"
    )]
    Initialization {
        bot_id: String,
        #[source]
        source: Error,
    },
}

#[derive(Clone)]
pub struct BotService {
    pool: SqlitePool,
    home: PathBuf,
    clients: AgentClientService,
    initialization_gate: Arc<tokio::sync::Mutex<()>>,
}

impl BotService {
    pub fn new(pool: SqlitePool, pontia_home: &Path, clients: ClientRegistry) -> Self {
        Self {
            clients: AgentClientService::new(pool.clone(), clients),
            pool,
            home: pontia_home.to_owned(),
            initialization_gate: Arc::default(),
        }
    }

    pub async fn create(
        &self,
        configuration: BotConfiguration,
        locale: Locale,
    ) -> std::result::Result<Bot, CreateError> {
        self.validate(&configuration).await?;
        let mut tx = self.pool.begin().await.map_err(Error::from)?;
        let bot: Bot = sqlx::query_as(
            "INSERT INTO bots (bot_id, name, description, avatar_shape, avatar_color, client_type, client_key, model, reasoning_effort)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(format!("bot_{}", uuid::Uuid::now_v7()))
        .bind(configuration.name.trim())
        .bind(normalize_description(configuration.description.as_deref()))
        .bind(configuration.avatar_shape.unwrap_or_default())
        .bind(configuration.avatar_color.unwrap_or_default())
        .bind(configuration.client_type)
        .bind(configuration.client_key)
        .bind(configuration.model)
        .bind(configuration.reasoning_effort)
        .fetch_one(&mut *tx)
        .await
        .map_err(Error::from)?;
        append_message(
            &mut *tx,
            &bot.bot_id,
            MessageKind::BotReply,
            &locale.greeting(&bot.name),
            None,
        )
        .await?;
        tx.commit().await.map_err(Error::from)?;
        self.initialize(&bot.bot_id)
            .await
            .map_err(|source| CreateError::Initialization {
                bot_id: bot.bot_id.clone(),
                source,
            })?;
        Ok(bot)
    }

    pub async fn get(&self, bot_id: &str) -> Result<Bot> {
        sqlx::query_as("SELECT * FROM bots WHERE bot_id = ?")
            .bind(bot_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Bot {bot_id} not found")))
    }

    pub async fn list(&self) -> Result<Vec<Bot>> {
        Ok(
            sqlx::query_as("SELECT * FROM bots ORDER BY created_at, bot_id")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn update(&self, bot_id: &str, configuration: BotConfiguration) -> Result<Bot> {
        self.validate(&configuration).await?;
        Ok(sqlx::query_as(
            "UPDATE bots SET name = ?, description = ?, client_type = ?, client_key = ?, model = ?, reasoning_effort = ?,
             avatar_shape = COALESCE(?, avatar_shape), avatar_color = COALESCE(?, avatar_color),
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE bot_id = ? RETURNING *",
        )
        .bind(configuration.name.trim())
        .bind(normalize_description(configuration.description.as_deref()))
        .bind(configuration.client_type)
        .bind(configuration.client_key)
        .bind(configuration.model)
        .bind(configuration.reasoning_effort)
        .bind(configuration.avatar_shape)
        .bind(configuration.avatar_color)
        .bind(bot_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub(crate) async fn set_description_if_empty(
        &self,
        bot_id: &str,
        session_id: &str,
        text: &str,
    ) -> Result<Bot> {
        let description = normalize_description(Some(text))
            .ok_or_else(|| Error::Domain("description cannot be empty".into()))?;
        if description.chars().count() > BOT_DESCRIPTION_LIMIT {
            return Err(Error::Domain(format!(
                "description exceeds {BOT_DESCRIPTION_LIMIT} characters"
            )));
        }
        let bot = sqlx::query_as(
            "UPDATE bots SET description = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE bot_id = ? AND current_session_id = ?
               AND (description IS NULL OR trim(description, char(9, 10, 11, 12, 13, 32)) = '')
             RETURNING *",
        )
        .bind(description)
        .bind(bot_id)
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(bot) = bot {
            return Ok(bot);
        }
        if self.get(bot_id).await?.current_session_id.as_deref() != Some(session_id) {
            return Err(Error::Conflict {
                code: "bot_session_required",
                message: IdentityError.to_string(),
            });
        }
        Err(Error::Conflict {
            code: "bot_description_already_set",
            message: "This Bot already has a description. Only the user can change it.".into(),
        })
    }

    /// Deletes the Bot, its chat and associations, and its directory, all or nothing in the database.
    pub(crate) async fn delete(&self, bot_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for table in ["bot_messages", "bot_helpers", "bot_notifications", "bots"] {
            sqlx::query(&format!("DELETE FROM {table} WHERE bot_id = ?"))
                .bind(bot_id)
                .execute(&mut *tx)
                .await?;
        }
        // Remove files before committing so a failure leaves the Bot in place to retry.
        match tokio::fs::remove_dir_all(self.home.join("bot").join(bot_id)).await {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn directory(&self, bot_id: &str) -> Result<PathBuf> {
        let bot = self.get(bot_id).await?;
        Ok(self.home.join("bot").join(bot.bot_id))
    }

    /// Shared launch directory for Sessions that only read Bot data, such as retrieval and Dreaming.
    pub async fn read_only_directory(&self) -> Result<PathBuf> {
        let directory = self.home.join("bot").join("readonly");
        tokio::fs::create_dir_all(&directory).await?;
        Ok(directory)
    }

    /// The conversation service owns Session lifecycle and routing around this pointer.
    pub async fn set_current_session(&self, bot_id: &str, session_id: Option<&str>) -> Result<Bot> {
        sqlx::query_as(
            "UPDATE bots SET current_session_id = ?, idle_since = NULL, idle_turn_id = NULL, awaiting_reply_after = NULL,
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE bot_id = ? RETURNING *",
        )
        .bind(session_id)
        .bind(bot_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Bot {bot_id} not found")))
    }

    pub(crate) async fn idle_timer(&self, bot_id: &str) -> Result<Option<BotIdleTimer>> {
        let row: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT idle_since, idle_turn_id FROM bots WHERE bot_id = ?")
                .bind(bot_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some((since, turn_id)) = row else {
            return Err(Error::NotFound(format!("Bot {bot_id} not found")));
        };
        Ok(since.map(|since| BotIdleTimer { since, turn_id }))
    }

    pub(crate) async fn set_idle_timer(
        &self,
        bot_id: &str,
        since: &str,
        turn_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE bots SET idle_since = ?, idle_turn_id = ? WHERE bot_id = ?")
            .bind(since)
            .bind(turn_id)
            .bind(bot_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn clear_idle_timer(&self, bot_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE bots SET idle_since = NULL, idle_turn_id = NULL
             WHERE bot_id = ? AND (idle_since IS NOT NULL OR idle_turn_id IS NOT NULL)",
        )
        .bind(bot_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn validate(&self, configuration: &BotConfiguration) -> Result<()> {
        for (field, value) in [
            ("name", &configuration.name),
            ("client_type", &configuration.client_type),
            ("client_key", &configuration.client_key),
            ("model", &configuration.model),
        ] {
            if value.trim().is_empty() {
                return Err(Error::Domain(format!("{field} cannot be empty")));
            }
        }
        if let Some(effort) = &configuration.reasoning_effort {
            return self
                .clients
                .validate_model_selection(
                    &configuration.client_type,
                    &configuration.client_key,
                    &configuration.model,
                    Some(effort),
                    &self.home,
                )
                .await;
        }
        self.clients
            .resolve(&configuration.client_type, &configuration.client_key)
            .await?;
        Ok(())
    }
}
