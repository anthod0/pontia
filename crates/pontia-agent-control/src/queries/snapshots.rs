use pontia_core::Result;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::ControlQueryService;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOwnership {
    pub application_id: String,
    pub reference: String,
}

#[derive(Debug, FromRow, Serialize)]
pub struct OwnedSession {
    pub session_id: String,
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct BusySessionSummary {
    pub session_id: String,
    pub title: Option<String>,
    pub role: Option<String>,
    pub created_at: String,
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
    pub is_control: bool,
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
    /// Reads busy Sessions from persisted state without probing clients.
    pub async fn busy_session_summaries(
        &self,
        session_ids: &[String],
    ) -> Result<Vec<BusySessionSummary>> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT session_id, title, role, created_at FROM sessions
             WHERE state = 'busy' AND session_id IN (",
        );
        let mut ids = query.separated(", ");
        for id in session_ids {
            ids.push_bind(id);
        }
        query.push(")");
        Ok(query.build_query_as().fetch_all(&self.pool).await?)
    }

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
        let runtimes = sqlx::query_as(
            "SELECT runtime_id, role, state, role IS ? AS is_control
             FROM session_runtimes WHERE session_id = ?",
        )
        .bind(self.clients.control_runtime_role(&client_type))
        .bind(session_id)
        .fetch_all(&mut *tx)
        .await?;
        let has_agent_binding =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_bindings WHERE session_id=?)")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?;
        let has_unsettled_input = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inbox_messages WHERE session_id=? AND (state IN ('pending','resuming','dispatching','unknown') OR (state='dispatched' AND turn_id IS NULL AND steer_target_turn_id IS NULL)))").bind(session_id).fetch_one(&mut *tx).await?;
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

    pub async fn session_ownership(&self, session_id: &str) -> Result<Option<SessionOwnership>> {
        let owner: Option<(String, String)> = sqlx::query_as(
            "SELECT application_id, application_ref FROM sessions WHERE session_id = ? AND application_id IS NOT NULL",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(owner.map(|(application_id, reference)| SessionOwnership {
            application_id,
            reference,
        }))
    }

    pub async fn verify_session_credential(
        &self,
        session_id: &str,
        environment_key: &str,
        credential: &str,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sessions
             WHERE session_id = ? AND json_extract(runtime_environment, ?) = ?)",
        )
        .bind(session_id)
        .bind(format!("$.{environment_key}"))
        .bind(credential)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn latest_owned_session(
        &self,
        application_id: &str,
        reference_prefix: &str,
    ) -> Result<Option<OwnedSession>> {
        Ok(sqlx::query_as(
            "SELECT session_id, workspace_ref AS workspace FROM sessions
             WHERE application_id = ? AND substr(application_ref, 1, length(?)) = ?
             ORDER BY application_ref DESC LIMIT 1",
        )
        .bind(application_id)
        .bind(reference_prefix)
        .bind(reference_prefix)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(crate) async fn application_launch_directory(
        &self,
        session_id: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT workspace_ref FROM sessions
             WHERE session_id = ? AND application_id IS NOT NULL AND workspace_id IS NULL AND workspace_ref IS NOT NULL",
        )
        .bind(session_id)
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
