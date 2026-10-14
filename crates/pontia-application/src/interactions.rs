use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionIntent {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractionOption {
    pub key: String,
    pub label: String,
    pub intent: InteractionIntent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionState {
    Pending,
    Resolving,
    Resolved,
    Expired,
}
impl InteractionState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resolving => "resolving",
            Self::Resolved => "resolved",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractionResponse {
    pub option_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Interaction {
    pub interaction_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    #[serde(rename = "type")]
    pub interaction_type: String,
    pub state: InteractionState,
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<InteractionOption>,
    pub response: Option<InteractionResponse>,
    pub requested_at: String,
    pub resolved_at: Option<String>,
}

pub struct ChoiceInteractionRequest {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<InteractionOption>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractionChanged {
    pub session_id: String,
    pub interaction_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum InteractionError {
    #[error("Interaction not found")]
    NotFound,
    #[error("Invalid interaction option")]
    InvalidOption,
    #[error("Interaction expired")]
    Expired,
    #[error("Interaction resolution in progress")]
    Resolving,
    #[error(transparent)]
    Other(#[from] Error),
}
impl From<sqlx::Error> for InteractionError {
    fn from(value: sqlx::Error) -> Self {
        Self::Other(value.into())
    }
}

#[derive(FromRow)]
struct InteractionRow {
    interaction_id: String,
    session_id: String,
    turn_id: Option<String>,
    r#type: String,
    title: String,
    description: Option<String>,
    options: String,
    state: String,
    response_option_key: Option<String>,
    requested_at: String,
    resolved_at: Option<String>,
}
impl InteractionRow {
    fn view(self) -> Result<Interaction> {
        Ok(Interaction {
            interaction_id: self.interaction_id,
            session_id: self.session_id,
            turn_id: self.turn_id,
            interaction_type: self.r#type,
            state: serde_json::from_value(serde_json::Value::String(self.state))?,
            title: self.title,
            description: self.description,
            options: serde_json::from_str(&self.options)?,
            response: self
                .response_option_key
                .map(|option_key| InteractionResponse { option_key }),
            requested_at: self.requested_at,
            resolved_at: self.resolved_at,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    session: String,
    state: Option<InteractionState>,
    time: String,
    id: String,
}

#[derive(Serialize)]
pub struct InteractionPage {
    pub interactions: Vec<Interaction>,
    pub next_cursor: Option<String>,
}

#[derive(Clone)]
pub struct InteractionService {
    pool: SqlitePool,
    changed: broadcast::Sender<InteractionChanged>,
}
impl InteractionService {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            changed: broadcast::channel(1024).0,
        }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<InteractionChanged> {
        self.changed.subscribe()
    }
    fn notify(&self, session: &str, id: &str) {
        let _ = self.changed.send(InteractionChanged {
            session_id: session.into(),
            interaction_id: id.into(),
        });
    }
    pub async fn expire_leftovers(&self) -> Result<()> {
        sqlx::query(
            "UPDATE interactions SET state='expired' WHERE state IN ('pending','resolving')",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
    pub async fn register(&self, request: ChoiceInteractionRequest) -> Result<Interaction> {
        let mut keys = std::collections::HashSet::new();
        if request.title.trim().is_empty()
            || request.options.is_empty()
            || request.options.iter().any(|o| {
                o.key.trim().is_empty() || o.label.trim().is_empty() || !keys.insert(&o.key)
            })
        {
            return Err(Error::Domain("Invalid choice interaction".into()));
        }
        // Do not attach an interaction to a Turn owned by another Session.
        if let Some(turn) = &request.turn_id {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM turns WHERE turn_id=? AND session_id=?)",
            )
            .bind(turn)
            .bind(&request.session_id)
            .fetch_one(&self.pool)
            .await?;
            if !exists {
                return Err(Error::Domain(
                    "Interaction Turn does not belong to Session".into(),
                ));
            }
        }
        let id = format!("interaction_{}", uuid::Uuid::now_v7());
        let now = timestamp();
        sqlx::query("INSERT INTO interactions(interaction_id,session_id,turn_id,type,title,description,options,state,requested_at) VALUES(?,?,?,'choice',?,?,?,'pending',?)")
            .bind(&id).bind(&request.session_id).bind(&request.turn_id).bind(&request.title).bind(&request.description).bind(serde_json::to_string(&request.options)?).bind(&now).execute(&self.pool).await?;
        self.notify(&request.session_id, &id);
        Ok(Interaction {
            interaction_id: id,
            session_id: request.session_id,
            turn_id: request.turn_id,
            interaction_type: "choice".into(),
            state: InteractionState::Pending,
            title: request.title,
            description: request.description,
            options: request.options,
            response: None,
            requested_at: now,
            resolved_at: None,
        })
    }
    pub async fn get(
        &self,
        session: &str,
        id: &str,
    ) -> std::result::Result<Interaction, InteractionError> {
        sqlx::query_as::<_, InteractionRow>(
            "SELECT * FROM interactions WHERE session_id=? AND interaction_id=?",
        )
        .bind(session)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(InteractionError::NotFound)?
        .view()
        .map_err(Into::into)
    }
    pub async fn list(
        &self,
        session: &str,
        state: Option<InteractionState>,
        limit: i64,
        cursor: Option<&str>,
    ) -> std::result::Result<InteractionPage, InteractionError> {
        if !(1..=200).contains(&limit) {
            return Err(Error::Domain("Invalid interaction limit".into()).into());
        }
        let cursor: Option<Cursor> = cursor
            .map(|value| {
                URL_SAFE_NO_PAD
                    .decode(value)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Cursor>(&bytes).ok())
                    .filter(|c| {
                        c.session == session
                            && c.state == state
                            && time::OffsetDateTime::parse(
                                &c.time,
                                &time::format_description::well_known::Rfc3339,
                            )
                            .is_ok()
                            && c.id
                                .strip_prefix("interaction_")
                                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
                    })
                    .ok_or_else(|| Error::Domain("Invalid interaction cursor".into()))
            })
            .transpose()?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id=?)")
                .bind(session)
                .fetch_one(&self.pool)
                .await?;
        if !exists {
            return Err(InteractionError::NotFound);
        }
        let ascending = state == Some(InteractionState::Pending);
        let direction = if ascending { "ASC" } else { "DESC" };
        let compare = if ascending { ">" } else { "<" };
        let sql = format!(
            "SELECT * FROM interactions WHERE session_id=? AND (? IS NULL OR state=?) AND (? IS NULL OR (requested_at,interaction_id) {compare} (?,?)) ORDER BY requested_at {direction},interaction_id {direction} LIMIT ?"
        );
        let mut interactions = sqlx::query_as::<_, InteractionRow>(&sql)
            .bind(session)
            .bind(state.as_ref().map(InteractionState::as_str))
            .bind(state.as_ref().map(InteractionState::as_str))
            .bind(cursor.as_ref().map(|c| &c.time))
            .bind(cursor.as_ref().map(|c| &c.time))
            .bind(cursor.as_ref().map(|c| &c.id))
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(InteractionRow::view)
            .collect::<Result<Vec<_>>>()?;
        let next_cursor = if interactions.len() > limit as usize {
            interactions.truncate(limit as usize);
            let last = interactions.last().expect("positive limit");
            Some(
                URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(&Cursor {
                        session: session.into(),
                        state,
                        time: last.requested_at.clone(),
                        id: last.interaction_id.clone(),
                    })
                    .map_err(Error::from)?,
                ),
            )
        } else {
            None
        };
        Ok(InteractionPage {
            interactions,
            next_cursor,
        })
    }
    pub async fn expire(&self, session: &str, id: &str) -> Result<()> {
        if sqlx::query("UPDATE interactions SET state='expired' WHERE session_id=? AND interaction_id=? AND state IN ('pending','resolving')").bind(session).bind(id).execute(&self.pool).await?.rows_affected() > 0 { self.notify(session,id); }
        Ok(())
    }
    pub async fn respond(
        &self,
        app: &crate::AppState,
        session: &str,
        id: &str,
        option: &str,
    ) -> std::result::Result<Interaction, InteractionError> {
        let interaction = self.get(session, id).await?;
        match interaction.state {
            InteractionState::Resolved => return Ok(interaction),
            InteractionState::Expired => return Err(InteractionError::Expired),
            InteractionState::Resolving => return Err(InteractionError::Resolving),
            InteractionState::Pending => {}
        }
        if !interaction.options.iter().any(|o| o.key == option) {
            return Err(InteractionError::InvalidOption);
        }
        let claimed = sqlx::query("UPDATE interactions SET state='resolving' WHERE session_id=? AND interaction_id=? AND state='pending'").bind(session).bind(id).execute(&self.pool).await?.rows_affected() > 0;
        if !claimed {
            return match self.get(session, id).await?.state {
                InteractionState::Resolved => self.get(session, id).await,
                InteractionState::Expired => Err(InteractionError::Expired),
                _ => Err(InteractionError::Resolving),
            };
        }
        self.notify(session, id);
        let result = async {
            let client: String =
                sqlx::query_scalar("SELECT client_type FROM sessions WHERE session_id=?")
                    .bind(session)
                    .fetch_one(&self.pool)
                    .await?;
            let clients = app.clients();
            let resolver = clients
                .get(&client)
                .and_then(|r| r.interactions.as_ref())
                .ok_or_else(|| {
                    Error::CapabilityUnavailable("Interaction resolution unsupported".into())
                })?;
            resolver
                .resolve(app.event_ingest_service(), session, id, option)
                .await
        }
        .await;
        if let Err(error) = result {
            self.expire(session, id).await?;
            tracing::warn!(%session, interaction_id = id, %error, "Interaction response unavailable; native response will not be replayed");
            return match error {
                Error::Database(_) | Error::Serialization(_) => Err(InteractionError::Other(error)),
                _ => Err(InteractionError::Expired),
            };
        }
        let now = timestamp();
        let updated = match sqlx::query("UPDATE interactions SET state='resolved',response_option_key=?,resolved_at=? WHERE session_id=? AND interaction_id=? AND state='resolving'").bind(option).bind(now).bind(session).bind(id).execute(&self.pool).await {
            Ok(result) => result.rows_affected(),
            Err(error) => {
                // No replay or background coordinator: release the claim if storage
                // is still writable, otherwise restart recovery expires the leftover.
                if let Err(expiration) = self.expire(session, id).await {
                    tracing::error!(interaction_id = id, %expiration, "Could not expire Interaction after completion persistence failed");
                }
                return Err(error.into());
            }
        };
        if updated == 0 {
            return Err(InteractionError::Expired);
        }
        self.notify(session, id);
        self.get(session, id).await
    }
}
fn timestamp() -> String {
    let format = time::format_description::parse(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:9]Z",
    )
    .expect("valid UTC timestamp format");
    time::OffsetDateTime::now_utc()
        .format(&format)
        .expect("UTC contains all timestamp components")
}
