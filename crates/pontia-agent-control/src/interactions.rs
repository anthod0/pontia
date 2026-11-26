use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, broadcast},
    time::Instant,
};

const RECONNECT_GRACE: Duration = Duration::from_secs(60);

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
pub enum InteractionType {
    Choice,
    MultiSelect,
    Text,
}

impl InteractionType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Choice => "choice",
            Self::MultiSelect => "multi_select",
            Self::Text => "text",
        }
    }
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
#[serde(untagged, deny_unknown_fields)]
pub enum InteractionResponse {
    Choice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        option_key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom_text: Option<String>,
    },
    MultiSelect {
        option_keys: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom_text: Option<String>,
    },
    Text {
        text: String,
    },
}

impl InteractionResponse {
    pub fn choice(option_key: impl Into<String>) -> Self {
        Self::Choice {
            option_key: Some(option_key.into()),
            custom_text: None,
        }
    }

    pub fn choice_option_key(&self) -> Result<&str> {
        match self {
            Self::Choice {
                option_key: Some(option_key),
                custom_text: None,
            } => Ok(option_key),
            _ => Err(Error::Domain(
                "Expected a choice interaction response".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Interaction {
    pub interaction_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    #[serde(rename = "type")]
    pub interaction_type: InteractionType,
    pub state: InteractionState,
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<InteractionOption>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allows_custom_text: bool,
    pub response: Option<InteractionResponse>,
    pub requested_at: String,
    pub resolved_at: Option<String>,
}

/// Hash authorization JSON with sorted object keys and the original array order.
pub fn authorization_digest(mut content: serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    content.sort_all_objects();
    format!("{:x}", Sha256::digest(content.to_string().as_bytes()))
}

pub fn transient_storage_error(error: &Error) -> bool {
    match error {
        Error::Database(sqlx::Error::PoolTimedOut | sqlx::Error::Io(_)) => true,
        Error::Database(sqlx::Error::Database(db)) => {
            // SQLite extended result codes retain the primary code in their low byte.
            // BUSY, LOCKED, IOERR, FULL and CANTOPEN can recover without changing the request.
            db.code()
                .and_then(|code| code.parse::<u32>().ok())
                .is_some_and(|code| matches!(code & 0xff, 5 | 6 | 10 | 13 | 14))
        }
        _ => false,
    }
}

#[derive(Clone)]
pub struct ChoiceInteractionRequest {
    pub report_key: String,
    pub authorization_digest: String,
    pub blocked: Option<InteractionBlock>,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub options: Vec<InteractionOption>,
}

#[derive(Clone)]
pub struct InteractionRequest {
    pub report_key: String,
    pub authorization_digest: String,
    pub blocked: Option<InteractionBlock>,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub input: InteractionInput,
}

#[derive(Clone)]
pub enum InteractionInput {
    Choice {
        options: Vec<InteractionOption>,
        allows_custom_text: bool,
    },
    MultiSelect {
        options: Vec<InteractionOption>,
        allows_custom_text: bool,
    },
    Text,
}

impl InteractionInput {
    fn interaction_type(&self) -> InteractionType {
        match self {
            Self::Choice { .. } => InteractionType::Choice,
            Self::MultiSelect { .. } => InteractionType::MultiSelect,
            Self::Text => InteractionType::Text,
        }
    }

    fn options(&self) -> &[InteractionOption] {
        match self {
            Self::Choice { options, .. } | Self::MultiSelect { options, .. } => options,
            Self::Text => &[],
        }
    }

    fn allows_custom_text(&self) -> bool {
        match self {
            Self::Choice {
                allows_custom_text, ..
            }
            | Self::MultiSelect {
                allows_custom_text, ..
            } => *allows_custom_text,
            Self::Text => false,
        }
    }
}

impl From<ChoiceInteractionRequest> for InteractionRequest {
    fn from(request: ChoiceInteractionRequest) -> Self {
        Self {
            report_key: request.report_key,
            authorization_digest: request.authorization_digest,
            blocked: request.blocked,
            session_id: request.session_id,
            turn_id: request.turn_id,
            title: request.title,
            description: request.description,
            input: InteractionInput::Choice {
                options: request.options,
                allows_custom_text: false,
            },
        }
    }
}

#[derive(Clone)]
pub struct InteractionBlock {
    pub key_prefix: String,
    pub runtime_id: Option<String>,
    pub turn_id: Option<String>,
}

impl InteractionBlock {
    fn fact(&self, session: &str, id: &str) -> crate::ReportedFact {
        let mut data = serde_json::json!({"interaction_id": id});
        if let Some(runtime) = &self.runtime_id {
            data["runtime_id"] = serde_json::json!(runtime);
        }
        crate::ReportedFact {
            session_id: session.into(),
            turn_id: self.turn_id.clone(),
            report_key: Some(format!("{}:{id}:interaction.blocked", self.key_prefix)),
            fact_type: pontia_core::domain::EventType::InteractionBlocked,
            data,
        }
    }
}

impl InteractionRequest {
    pub(crate) async fn insert(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        id: &str,
    ) -> Result<()> {
        if let Some(turn) = &self.turn_id {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM turns WHERE turn_id=? AND session_id=?)",
            )
            .bind(turn)
            .bind(&self.session_id)
            .fetch_one(&mut **tx)
            .await?;
            if !exists {
                return Err(Error::Domain(
                    "Interaction Turn does not belong to Session".into(),
                ));
            }
        }
        sqlx::query("INSERT INTO interactions(interaction_id,session_id,turn_id,type,title,description,options,allows_custom_text,state,requested_at,report_key,authorization_digest) VALUES(?,?,?,?,?,?,?,?, 'pending',?,?,?)")
            .bind(id).bind(&self.session_id).bind(&self.turn_id).bind(self.input.interaction_type().as_str()).bind(&self.title).bind(&self.description)
            .bind(serde_json::to_string(self.input.options())?).bind(self.input.allows_custom_text()).bind(timestamp()).bind(&self.report_key).bind(&self.authorization_digest)
            .execute(&mut **tx).await?;
        Ok(())
    }
}

pub(crate) enum InteractionWrite<'a> {
    Register(&'a InteractionRequest, &'a str),
    Settle {
        id: &'a str,
        key: &'a str,
        outcome: &'a str,
    },
}

impl InteractionWrite<'_> {
    pub(crate) async fn apply(&self, tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
        match self {
            Self::Register(request, id) => request.insert(tx, id).await,
            Self::Settle { id, key, outcome } => {
                let updated = sqlx::query("UPDATE interactions SET settlement_report_key=?, settlement_outcome=?, state=CASE WHEN ?='expired' AND state IN ('pending','resolving') THEN 'expired' ELSE state END WHERE interaction_id=? AND (settlement_report_key IS NULL OR (settlement_report_key=? AND settlement_outcome=?))")
                    .bind(*key).bind(*outcome).bind(*outcome).bind(*id).bind(*key).bind(*outcome).execute(&mut **tx).await;
                match updated {
                    Ok(result) if result.rows_affected() > 0 => Ok(()),
                    Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(
                        Error::StateConflict("Interaction settlement key already used".into()),
                    ),
                    Err(error) => Err(error.into()),
                    Ok(_) => Err(Error::StateConflict(
                        "Conflicting interaction settlement".into(),
                    )),
                }
            }
        }
    }
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
    authorization_digest: Option<String>,
    interaction_id: String,
    session_id: String,
    turn_id: Option<String>,
    r#type: String,
    title: String,
    description: Option<String>,
    options: String,
    state: String,
    allows_custom_text: bool,
    response: Option<String>,
    requested_at: String,
    resolved_at: Option<String>,
}
impl InteractionRow {
    fn view(self) -> Result<Interaction> {
        Ok(Interaction {
            interaction_id: self.interaction_id,
            session_id: self.session_id,
            turn_id: self.turn_id,
            interaction_type: serde_json::from_value(serde_json::Value::String(self.r#type))?,
            state: serde_json::from_value(serde_json::Value::String(self.state))?,
            title: self.title,
            description: self.description,
            options: serde_json::from_str(&self.options)?,
            allows_custom_text: self.allows_custom_text,
            response: self
                .response
                .map(|response| serde_json::from_str(&response))
                .transpose()?,
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
    events: crate::EventIngestService,
    changed: broadcast::Sender<InteractionChanged>,
    awaiting_reconnect: Arc<Mutex<HashMap<String, Instant>>>,
    shutdown: tokio::sync::watch::Receiver<bool>,
}
impl InteractionService {
    pub(crate) fn new(
        events: crate::EventIngestService,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Self {
        Self {
            pool: events.db(),
            events,
            changed: broadcast::channel(1024).0,
            awaiting_reconnect: Default::default(),
            shutdown,
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
    pub async fn recover_pending(&self) -> Result<()> {
        sqlx::query("UPDATE interactions SET state='expired' WHERE state='resolving'")
            .execute(&self.pool)
            .await?;
        let pending: Vec<(String, String)> = sqlx::query_as(
            "SELECT session_id, interaction_id FROM interactions WHERE state='pending'",
        )
        .fetch_all(&self.pool)
        .await?;
        self.await_reconnect(pending).await;
        Ok(())
    }

    pub async fn await_reconnect(&self, pending: Vec<(String, String)>) {
        if pending.is_empty() {
            return;
        }
        let deadline = Instant::now() + RECONNECT_GRACE;
        let mut awaiting = self.awaiting_reconnect.lock().await;
        awaiting.extend(pending.iter().map(|(_, id)| (id.clone(), deadline)));
        let service = self.clone();
        tokio::spawn(async move {
            let mut shutdown = service.shutdown.clone();
            if *shutdown.borrow() {
                return;
            }
            tokio::select! {
                _ = shutdown.changed() => return,
                _ = tokio::time::sleep_until(deadline) => {}
            }
            for (session, id) in pending {
                loop {
                    let mut awaiting = service.awaiting_reconnect.lock().await;
                    if *shutdown.borrow() || awaiting.get(&id) != Some(&deadline) {
                        break;
                    }
                    match service.expire(&session, &id).await {
                        Ok(()) => {
                            awaiting.remove(&id);
                            break;
                        }
                        Err(error) if transient_storage_error(&error) => {
                            tracing::warn!(%session, %id, %error, "Retrying interaction expiry");
                        }
                        Err(error) => {
                            tracing::error!(%session, %id, %error, "Interaction expiry failed");
                            break;
                        }
                    }
                    drop(awaiting);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        });
    }
    pub async fn register(&self, request: impl Into<InteractionRequest>) -> Result<Interaction> {
        let request = request.into();
        if request.authorization_digest.len() != 64
            || !request
                .authorization_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Domain(
                "Invalid interaction authorization digest".into(),
            ));
        }
        if request.report_key.trim().is_empty() {
            return Err(Error::Domain("Missing interaction report_key".into()));
        }
        {
            let mut awaiting = self.awaiting_reconnect.lock().await;
            if let Some(existing) = self.registered(&request).await? {
                awaiting.remove(&existing.interaction_id);
                return Ok(existing);
            }
        }
        let mut keys = std::collections::HashSet::new();
        let options = request.input.options();
        if request.title.trim().is_empty()
            || (!matches!(&request.input, InteractionInput::Text) && options.is_empty())
            || options.iter().any(|option| {
                option.key.trim().is_empty()
                    || option.label.trim().is_empty()
                    || !keys.insert(&option.key)
            })
        {
            return Err(Error::Domain("Invalid interaction".into()));
        }
        let id = format!("interaction_{}", uuid::Uuid::now_v7());
        let result = if let Some(block) = &request.blocked {
            self.events
                .write_interaction(
                    block.fact(&request.session_id, &id),
                    InteractionWrite::Register(&request, &id),
                )
                .await
        } else {
            let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
            match request.insert(&mut tx, &id).await {
                Ok(()) => tx.commit().await.map_err(Into::into),
                Err(error) => Err(error),
            }
        };
        if let Err(error) = result {
            // A concurrent registration can win the unique key while this writer waits.
            return self.registered(&request).await?.ok_or(error);
        }
        self.notify(&request.session_id, &id);
        sqlx::query_as::<_, InteractionRow>("SELECT * FROM interactions WHERE interaction_id=?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?
            .view()
    }

    pub async fn settle(
        &self,
        session: &str,
        registration_key: &str,
        digest: &str,
        key: &str,
        outcome: &str,
    ) -> Result<String> {
        let (interaction, original) = self
            .registration(session, registration_key)
            .await?
            .ok_or_else(|| Error::StateConflict("Unknown interaction registration".into()))?;
        if original != digest
            || key.is_empty()
            || !matches!(outcome, "resolved" | "expired" | "delivered")
        {
            return Err(Error::StateConflict(
                "Invalid interaction settlement".into(),
            ));
        }
        let id = interaction.interaction_id;
        let previous: Option<(String,String)> = sqlx::query_as("SELECT settlement_report_key,settlement_outcome FROM interactions WHERE interaction_id=? AND settlement_report_key IS NOT NULL")
            .bind(&id).fetch_optional(&self.pool).await?;
        if let Some((stored_key, stored_outcome)) = previous {
            if stored_key == key && stored_outcome == outcome {
                return Ok(id);
            }
            return Err(Error::StateConflict(
                "Conflicting interaction settlement".into(),
            ));
        }
        let block: Option<(String, Option<String>, String)> = sqlx::query_as("SELECT report_key,turn_id,payload FROM events WHERE session_id=? AND event_type='interaction.blocked' AND json_extract(payload,'$.interaction_id')=?")
            .bind(session).bind(&id).fetch_optional(&self.pool).await?;
        let write = InteractionWrite::Settle {
            id: &id,
            key,
            outcome,
        };
        if let Some((blocked_key, turn_id, payload)) = block {
            let prefix = blocked_key
                .strip_suffix(":interaction.blocked")
                .ok_or_else(|| Error::Domain("Invalid blocked event key".into()))?;
            self.events
                .write_interaction(
                    crate::ReportedFact {
                        session_id: session.into(),
                        turn_id,
                        report_key: Some(format!("{prefix}:interaction.unblocked")),
                        fact_type: pontia_core::domain::EventType::InteractionUnblocked,
                        data: serde_json::from_str(&payload)?,
                    },
                    write,
                )
                .await?;
        } else {
            let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
            write.apply(&mut tx).await?;
            tx.commit().await?;
        }
        self.notify(session, &id);
        Ok(id)
    }

    pub async fn registration(
        &self,
        session: &str,
        key: &str,
    ) -> Result<Option<(Interaction, String)>> {
        let Some(mut row) = sqlx::query_as::<_, InteractionRow>(
            "SELECT * FROM interactions WHERE session_id=? AND report_key=?",
        )
        .bind(session)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        let digest = row
            .authorization_digest
            .take()
            .ok_or_else(|| Error::Domain("Missing stored authorization digest".into()))?;
        Ok(Some((row.view()?, digest)))
    }

    pub async fn registered(&self, request: &InteractionRequest) -> Result<Option<Interaction>> {
        let Some((stored, digest)) = self
            .registration(&request.session_id, &request.report_key)
            .await?
        else {
            return Ok(None);
        };
        let block: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT report_key, turn_id, json_extract(payload,'$.runtime_id') FROM events
             WHERE session_id=? AND event_type='interaction.blocked'
               AND json_extract(payload,'$.interaction_id')=?",
        )
        .bind(&request.session_id)
        .bind(&stored.interaction_id)
        .fetch_optional(&self.pool)
        .await?;
        let expected = request.blocked.as_ref().map(|b| {
            (
                format!(
                    "{}:{}:interaction.blocked",
                    b.key_prefix, stored.interaction_id
                ),
                b.turn_id.clone(),
                b.runtime_id.clone(),
            )
        });
        if digest != request.authorization_digest
            || stored.turn_id != request.turn_id
            || stored.title != request.title
            || stored.description != request.description
            || stored.interaction_type != request.input.interaction_type()
            || stored.options != request.input.options()
            || stored.allows_custom_text != request.input.allows_custom_text()
            || block != expected
        {
            return Err(Error::StateConflict(format!(
                "Different interaction content for report_key {}",
                request.report_key
            )));
        }
        Ok(Some(stored))
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
        app: &crate::AgentControl,
        session: &str,
        id: &str,
        option: &str,
    ) -> std::result::Result<Interaction, InteractionError> {
        self.respond_with(app, session, id, &InteractionResponse::choice(option))
            .await
    }

    pub async fn respond_with(
        &self,
        app: &crate::AgentControl,
        session: &str,
        id: &str,
        response: &InteractionResponse,
    ) -> std::result::Result<Interaction, InteractionError> {
        let interaction = self.get(session, id).await?;
        match interaction.state {
            InteractionState::Resolved => return Ok(interaction),
            InteractionState::Expired => return Err(InteractionError::Expired),
            InteractionState::Resolving => return Err(InteractionError::Resolving),
            InteractionState::Pending => {}
        }
        if self.awaiting_reconnect.lock().await.contains_key(id) {
            return Err(Error::CapabilityUnavailable(
                "Waiting for the client to confirm this interaction".into(),
            )
            .into());
        }
        if !valid_response(&interaction, response) {
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
                .resolve(app.event_ingest_service(), session, id, response)
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
        let updated = match sqlx::query("UPDATE interactions SET state='resolved',response=?,resolved_at=? WHERE session_id=? AND interaction_id=? AND state='resolving'").bind(serde_json::to_string(response).map_err(Error::from)?).bind(now).bind(session).bind(id).execute(&self.pool).await {
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

fn valid_response(interaction: &Interaction, response: &InteractionResponse) -> bool {
    let valid_custom_text = |text: &Option<String>| {
        text.as_ref()
            .is_some_and(|text| interaction.allows_custom_text && !text.trim().is_empty())
    };
    match (&interaction.interaction_type, response) {
        (
            InteractionType::Choice,
            InteractionResponse::Choice {
                option_key,
                custom_text,
            },
        ) => {
            let option_valid = option_key
                .as_ref()
                .is_some_and(|key| interaction.options.iter().any(|option| option.key == *key));
            (option_key.is_none() || option_valid)
                && (custom_text.is_none() || valid_custom_text(custom_text))
                && (option_valid || valid_custom_text(custom_text))
        }
        (
            InteractionType::MultiSelect,
            InteractionResponse::MultiSelect {
                option_keys,
                custom_text,
            },
        ) => {
            let keys: std::collections::HashSet<_> = option_keys.iter().collect();
            keys.len() == option_keys.len()
                && option_keys
                    .iter()
                    .all(|key| interaction.options.iter().any(|option| option.key == *key))
                && (custom_text.is_none() || valid_custom_text(custom_text))
                && (!option_keys.is_empty() || valid_custom_text(custom_text))
        }
        (InteractionType::Text, InteractionResponse::Text { text }) => !text.trim().is_empty(),
        _ => false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn interaction(interaction_type: InteractionType, allows_custom_text: bool) -> Interaction {
        Interaction {
            interaction_id: "interaction_test".into(),
            session_id: "session_test".into(),
            turn_id: None,
            interaction_type,
            state: InteractionState::Pending,
            title: "Question".into(),
            description: None,
            options: vec![
                InteractionOption {
                    key: "first".into(),
                    label: "First".into(),
                    intent: InteractionIntent::Positive,
                },
                InteractionOption {
                    key: "second".into(),
                    label: "Second".into(),
                    intent: InteractionIntent::Negative,
                },
            ],
            allows_custom_text,
            response: None,
            requested_at: "2026-11-27T00:00:00Z".into(),
            resolved_at: None,
        }
    }

    #[test]
    fn choice_response_preserves_the_existing_json_shape() {
        let response: InteractionResponse =
            serde_json::from_value(json!({"option_key": "first"})).unwrap();
        assert_eq!(response, InteractionResponse::choice("first"));
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            json!({"option_key": "first"})
        );
    }

    #[test]
    fn extended_responses_round_trip() {
        for value in [
            json!({"custom_text": "Something else"}),
            json!({"option_key": "first", "custom_text": "With details"}),
            json!({"option_keys": ["first", "second"], "custom_text": "Other"}),
            json!({"text": "A free-form answer"}),
        ] {
            let response: InteractionResponse = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(response).unwrap(), value);
        }
    }

    #[test]
    fn interaction_types_round_trip_without_changing_existing_choice_json() {
        let choice = serde_json::to_value(interaction(InteractionType::Choice, false)).unwrap();
        assert_eq!(choice["type"], "choice");
        assert!(choice.get("allows_custom_text").is_none());
        let decoded: Interaction = serde_json::from_value(choice).unwrap();
        assert_eq!(decoded.interaction_type, InteractionType::Choice);
        assert!(!decoded.allows_custom_text);

        let multi = serde_json::to_value(interaction(InteractionType::MultiSelect, true)).unwrap();
        assert_eq!(multi["type"], "multi_select");
        assert_eq!(multi["allows_custom_text"], true);
    }

    #[test]
    fn validates_choice_responses_against_options_and_custom_text_capability() {
        let choice = interaction(InteractionType::Choice, true);
        assert!(valid_response(
            &choice,
            &InteractionResponse::choice("first")
        ));
        assert!(valid_response(
            &choice,
            &InteractionResponse::Choice {
                option_key: None,
                custom_text: Some("Another answer".into()),
            }
        ));
        assert!(!valid_response(
            &choice,
            &InteractionResponse::choice("missing")
        ));
        assert!(!valid_response(
            &interaction(InteractionType::Choice, false),
            &InteractionResponse::Choice {
                option_key: None,
                custom_text: Some("Another answer".into()),
            }
        ));
    }

    #[test]
    fn validates_multi_select_responses_as_unique_known_choices() {
        let multi = interaction(InteractionType::MultiSelect, true);
        assert!(valid_response(
            &multi,
            &InteractionResponse::MultiSelect {
                option_keys: vec!["first".into(), "second".into()],
                custom_text: None,
            }
        ));
        assert!(valid_response(
            &multi,
            &InteractionResponse::MultiSelect {
                option_keys: vec![],
                custom_text: Some("Another answer".into()),
            }
        ));
        assert!(!valid_response(
            &multi,
            &InteractionResponse::MultiSelect {
                option_keys: vec!["first".into(), "first".into()],
                custom_text: None,
            }
        ));
        assert!(!valid_response(
            &multi,
            &InteractionResponse::MultiSelect {
                option_keys: vec![],
                custom_text: None,
            }
        ));
    }

    #[test]
    fn validates_text_responses_as_non_blank_text() {
        let text = interaction(InteractionType::Text, false);
        assert!(valid_response(
            &text,
            &InteractionResponse::Text {
                text: "Answer".into()
            }
        ));
        assert!(!valid_response(
            &text,
            &InteractionResponse::Text { text: "  ".into() }
        ));
        assert!(!valid_response(
            &text,
            &InteractionResponse::choice("first")
        ));
    }
}
