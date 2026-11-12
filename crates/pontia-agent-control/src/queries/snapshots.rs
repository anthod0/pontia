use pontia_core::Result;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::ControlQueryService;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOwnership {
    pub application_id: String,
    pub reference: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct TurnSnapshot {
    pub turn_id: String,
    pub state: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct RuntimeSnapshot {
    pub runtime_id: String,
    pub role: String,
    pub state: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct InboxSnapshot {
    pub message_id: String,
    pub state: String,
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub session_id: String,
    pub client_type: String,
    pub state: String,
    pub current_turn_id: Option<String>,
    pub event_position: i64,
    pub revision: i64,
    pub turns: Vec<TurnSnapshot>,
    pub runtimes: Vec<RuntimeSnapshot>,
    pub has_agent_binding: bool,
    pub has_unsettled_input: bool,
    pub inbox: Vec<InboxSnapshot>,
}

impl ControlQueryService {
    pub async fn verify_active_turn(
        &self,
        session_id: &str,
        runtime_id: &str,
    ) -> Result<SessionSnapshot> {
        let snapshot = self.session_snapshot(session_id).await?.ok_or_else(|| {
            pontia_core::Error::NotFound(format!("Session {session_id} not found"))
        })?;
        if matches!(snapshot.state.as_str(), "exited" | "error")
            || !snapshot.runtimes.iter().any(|runtime| {
                runtime.runtime_id == runtime_id
                    && matches!(runtime.state.as_str(), "starting" | "running")
            })
            || snapshot
                .turns
                .iter()
                .filter(|turn| matches!(turn.state.as_str(), "queued" | "running"))
                .count()
                != 1
        {
            return Err(pontia_core::Error::StateConflict(
                "Expected Runtime and one active Turn are no longer available".into(),
            ));
        }
        Ok(snapshot)
    }

    /// Reads a consistent persisted snapshot without probing a client or its filesystem.
    pub async fn session_snapshot(&self, session_id: &str) -> Result<Option<SessionSnapshot>> {
        let mut tx = self.pool.begin().await?;
        let row: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT client_type,state,current_turn_id FROM sessions WHERE session_id=?",
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((client_type, state, current_turn_id)) = row else {
            return Ok(None);
        };
        let event_position =
            sqlx::query_scalar("SELECT COALESCE(MAX(position),0) FROM domain_event_positions")
                .fetch_one(&mut *tx)
                .await?;
        let revision = sqlx::query_scalar("SELECT revision FROM control_state_revision WHERE id=1")
            .fetch_one(&mut *tx)
            .await?;
        let turns = sqlx::query_as("SELECT turn_id,state FROM turns WHERE session_id=?")
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await?;
        let runtimes =
            sqlx::query_as("SELECT runtime_id,role,state FROM session_runtimes WHERE session_id=?")
                .bind(session_id)
                .fetch_all(&mut *tx)
                .await?;
        let has_agent_binding =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_bindings WHERE session_id=?)")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?;
        let has_unsettled_input = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inbox_messages WHERE session_id=? AND (state IN ('pending','resuming','dispatching','unknown') OR (state='dispatched' AND turn_id IS NULL)))").bind(session_id).fetch_one(&mut *tx).await?;
        let inbox = sqlx::query_as(
            "SELECT message_id,state,turn_id FROM inbox_messages WHERE session_id=?",
        )
        .bind(session_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(SessionSnapshot {
            session_id: session_id.into(),
            client_type,
            state,
            current_turn_id,
            event_position,
            revision,
            turns,
            runtimes,
            has_agent_binding,
            has_unsettled_input,
            inbox,
        }))
    }

    pub async fn find_session_by_ownership(
        &self,
        ownership: &SessionOwnership,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT session_id FROM sessions WHERE application_id=? AND application_ref=?",
        )
        .bind(&ownership.application_id)
        .bind(&ownership.reference)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_owned_sessions(&self, application_id: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT session_id FROM sessions WHERE application_id=? ORDER BY application_ref",
        )
        .bind(application_id)
        .fetch_all(&self.pool)
        .await?)
    }
}
