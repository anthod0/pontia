use super::WorkflowState;
use pontia_agent_control::{ControlQueryService, PositionedEvent};
use pontia_core::Result;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::Notify;

pub(super) struct WorkflowProjectionHandler {
    pub(super) pool: SqlitePool,
    pub(super) queries: ControlQueryService,
    pub(super) changed: Arc<Notify>,
}

impl WorkflowProjectionHandler {
    pub(super) async fn apply(&self, event: PositionedEvent) -> Result<()> {
        if event.application_id.as_deref() != Some("workflow") {
            let bound: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM workflow_nodes WHERE session_id=?)",
            )
            .bind(&event.session_id)
            .fetch_one(&self.pool)
            .await?;
            if !bound {
                return Ok(());
            }
        }
        let snapshot = self.queries.session_snapshot(&event.session_id).await?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(snapshot) = snapshot {
            WorkflowState::save_snapshot_in_tx(&mut tx, &snapshot).await?;
        }
        sqlx::query("INSERT INTO workflow_agent_events(position,event_id,session_id,turn_id,source,event_type,occurred_at,payload,created_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(event_id) DO NOTHING")
            .bind(event.position).bind(event.event_id).bind(event.session_id).bind(event.turn_id)
            .bind(event.source).bind(event.event_type).bind(event.occurred_at).bind(event.payload).bind(event.created_at).execute(&mut *tx).await?;
        tx.commit().await?;
        self.changed.notify_one();
        Ok(())
    }
}

impl WorkflowState {
    pub(super) async fn refresh_session(&self, session_id: &str) -> Result<()> {
        if let Some(snapshot) = self.queries.session_snapshot(session_id).await? {
            let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
            Self::save_snapshot_in_tx(&mut tx, &snapshot).await?;
            tx.commit().await?;
        }
        Ok(())
    }

    pub(super) async fn refresh_workflow_sessions(&self, workflow_id: &str) -> Result<()> {
        for node in self.list_nodes(workflow_id).await? {
            if let Some(id) = node.session_id {
                self.refresh_session(&id).await?;
            }
        }
        Ok(())
    }

    async fn save_snapshot_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        snapshot: &pontia_agent_control::SessionSnapshot,
    ) -> Result<()> {
        let changed = sqlx::query("INSERT INTO workflow_session_snapshots(session_id,client_type,state,current_turn_id,position,has_agent_binding,has_unsettled_input,revision) VALUES (?,?,?,?,?,?,?,?) ON CONFLICT(session_id) DO UPDATE SET client_type=excluded.client_type,state=excluded.state,current_turn_id=excluded.current_turn_id,position=MAX(workflow_session_snapshots.position,excluded.position),has_agent_binding=excluded.has_agent_binding,has_unsettled_input=excluded.has_unsettled_input,revision=excluded.revision WHERE excluded.revision>=workflow_session_snapshots.revision")
            .bind(&snapshot.session_id).bind(&snapshot.client_type).bind(&snapshot.state).bind(&snapshot.current_turn_id).bind(snapshot.event_position).bind(snapshot.has_agent_binding).bind(snapshot.has_unsettled_input).bind(snapshot.revision).execute(&mut **tx).await?;
        if changed.rows_affected() == 0 {
            return Ok(());
        }
        sqlx::query("DELETE FROM workflow_turn_snapshots WHERE session_id=?")
            .bind(&snapshot.session_id)
            .execute(&mut **tx)
            .await?;
        for turn in &snapshot.turns {
            sqlx::query(
                "INSERT INTO workflow_turn_snapshots(turn_id,session_id,state) VALUES (?,?,?)",
            )
            .bind(&turn.turn_id)
            .bind(&snapshot.session_id)
            .bind(&turn.state)
            .execute(&mut **tx)
            .await?;
        }
        sqlx::query("DELETE FROM workflow_runtime_snapshots WHERE session_id=?")
            .bind(&snapshot.session_id)
            .execute(&mut **tx)
            .await?;
        for runtime in &snapshot.runtimes {
            sqlx::query("INSERT INTO workflow_runtime_snapshots(runtime_id,session_id,role,state,is_control) VALUES (?,?,?,?,?)")
                .bind(&runtime.runtime_id).bind(&snapshot.session_id).bind(&runtime.role).bind(&runtime.state).bind(runtime.is_control).execute(&mut **tx).await?;
        }
        Ok(())
    }
}
