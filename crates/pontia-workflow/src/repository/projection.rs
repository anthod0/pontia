use super::SqliteWorkflowRepository;
use pontia_core::Result;

impl SqliteWorkflowRepository {
    /// Refreshes application read models through public control contracts.
    /// No scheduling side effects run while replay catches up.
    pub async fn synchronize(&self) -> Result<()> {
        let reader = self.queries.domain_events();
        loop {
            let after: i64 =
                sqlx::query_scalar("SELECT position FROM workflow_event_cursor WHERE id=1")
                    .fetch_one(&self.pool)
                    .await?;
            let page = reader.read_after(after, None, 256).await?;
            let ids: Vec<String> = sqlx::query_scalar("SELECT session_id FROM workflow_nodes WHERE session_id IS NOT NULL UNION SELECT replanner_session_id FROM workflow_patches WHERE replanner_session_id IS NOT NULL")
                .fetch_all(&self.pool).await?;

            let events: Vec<_> = page
                .events
                .into_iter()
                .filter(|event| {
                    event.application_id.as_deref() == Some("workflow")
                        || ids.contains(&event.session_id)
                })
                .collect();
            let affected: std::collections::BTreeSet<_> = events
                .iter()
                .map(|event| event.session_id.as_str())
                .collect();
            let mut snapshots = Vec::with_capacity(affected.len());
            for id in affected {
                if let Some(snapshot) = self.queries.session_snapshot(id).await? {
                    snapshots.push(snapshot);
                }
            }
            let target_position = snapshots
                .iter()
                .fold(page.head_position, |position, snapshot| {
                    position.max(snapshot.event_position)
                });
            let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
            for snapshot in &snapshots {
                Self::save_snapshot_in_tx(&mut tx, snapshot).await?;
            }
            for event in events {
                sqlx::query("INSERT INTO workflow_agent_events(position,event_id,session_id,turn_id,source,event_type,occurred_at,payload,created_at) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(event_id) DO NOTHING")
                    .bind(event.position).bind(event.event_id).bind(event.session_id).bind(event.turn_id)
                    .bind(event.source).bind(event.event_type).bind(event.occurred_at).bind(event.payload).bind(event.created_at).execute(&mut *tx).await?;
            }
            sqlx::query("UPDATE workflow_event_cursor SET position=MAX(position,?) WHERE id=1")
                .bind(page.next_position)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            if page.next_position >= target_position {
                break;
            }
        }
        Ok(())
    }

    pub(super) async fn verify_active_turn(
        &self,
        session_id: &str,
        runtime_id: &str,
    ) -> Result<()> {
        let snapshot = self
            .queries
            .verify_active_turn(session_id, runtime_id)
            .await?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        Self::save_snapshot_in_tx(&mut tx, &snapshot).await?;
        tx.commit().await?;
        Ok(())
    }

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
        let changed = sqlx::query("INSERT INTO workflow_session_snapshots(session_id,client_type,state,current_turn_id,position,has_agent_binding,revision) VALUES (?,?,?,?,?,?,?) ON CONFLICT(session_id) DO UPDATE SET client_type=excluded.client_type,state=excluded.state,current_turn_id=excluded.current_turn_id,position=MAX(workflow_session_snapshots.position,excluded.position),has_agent_binding=excluded.has_agent_binding,revision=excluded.revision WHERE excluded.revision>=workflow_session_snapshots.revision")
            .bind(&snapshot.session_id).bind(&snapshot.client_type).bind(&snapshot.state).bind(&snapshot.current_turn_id).bind(snapshot.event_position).bind(snapshot.has_agent_binding).bind(snapshot.revision).execute(&mut **tx).await?;
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
            sqlx::query("INSERT INTO workflow_runtime_snapshots(runtime_id,session_id,role,state) VALUES (?,?,?,?)")
                .bind(&runtime.runtime_id).bind(&snapshot.session_id).bind(&runtime.role).bind(&runtime.state).execute(&mut **tx).await?;
        }
        sqlx::query("DELETE FROM workflow_inbox_snapshots WHERE session_id=?")
            .bind(&snapshot.session_id)
            .execute(&mut **tx)
            .await?;
        for message in &snapshot.inbox {
            sqlx::query("INSERT INTO workflow_inbox_snapshots(message_id,session_id,state,turn_id) VALUES (?,?,?,?)")
                .bind(&message.message_id).bind(&snapshot.session_id).bind(&message.state).bind(&message.turn_id).execute(&mut **tx).await?;
        }
        Ok(())
    }
}
