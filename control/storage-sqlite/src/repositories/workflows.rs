use pontia_core::{Error, Result};
use sqlx::SqlitePool;

use crate::models::workflows::{WorkflowEventRow, WorkflowNodeRow, WorkflowPatchRow, WorkflowRow};

#[derive(Debug, Clone)]
pub struct CreateWorkflowRecord {
    pub workflow_id: String,
    pub title: String,
    pub cwd: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct CreateWorkflowNodeRecord {
    pub node_id: String,
    pub workflow_id: String,
    pub parent_node_id: Option<String>,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    pub inputs: String,
    pub output: String,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SqliteWorkflowRepository {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct RequestWorkflowPatchRecord {
    pub patch_id: String,
    pub session_id: String,
    pub runtime_instance_id: String,
    pub request_document_ref: String,
    pub request_size_bytes: i64,
    pub replanner_creation_token: String,
    pub event_id: String,
}

#[derive(Debug, Clone)]
pub struct BlockWorkflowPatchRecord {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub reason_document_ref: String,
    pub blocked_draft_ref: Option<String>,
    pub event_id: String,
}

struct RunningWorkflowTransition<'a> {
    workflow_id: &'a str,
    unsubmitted_node_id: Option<&'a str>,
    event_id: &'a str,
    state: &'a str,
    event_type: &'a str,
    failure_message: Option<&'a str>,
    payload: &'a str,
}

impl SqliteWorkflowRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_definition(
        &self,
        workflow: CreateWorkflowRecord,
        nodes: Vec<CreateWorkflowNodeRecord>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO workflows (workflow_id, title, cwd, state)
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(workflow.workflow_id)
        .bind(workflow.title)
        .bind(workflow.cwd)
        .bind(workflow.state)
        .execute(&mut *tx)
        .await?;
        for node in nodes {
            sqlx::query(
                r#"INSERT INTO workflow_nodes
                   (node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                    inputs, output, execution_profile_id, execution_profile_version)
                   VALUES (?, ?, ?, 'agent', ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(node.node_id)
            .bind(node.workflow_id)
            .bind(node.parent_node_id)
            .bind(node.phase)
            .bind(node.title)
            .bind(node.instructions)
            .bind(node.inputs)
            .bind(node.output)
            .bind(node.execution_profile_id)
            .bind(node.execution_profile_version)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn create_workflow(&self, workflow: CreateWorkflowRecord) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO workflows (workflow_id, title, cwd, state)
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(workflow.workflow_id)
        .bind(workflow.title)
        .bind(workflow.cwd)
        .bind(workflow.state)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_node(&self, node: CreateWorkflowNodeRecord) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO workflow_nodes
               (node_id, workflow_id, parent_node_id, node_type, phase, title, instructions, inputs,
                output, execution_profile_id, execution_profile_version)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(node.node_id)
        .bind(node.workflow_id)
        .bind(node.parent_node_id)
        .bind("agent")
        .bind(node.phase)
        .bind(node.title)
        .bind(node.instructions)
        .bind(node.inputs)
        .bind(node.output)
        .bind(node.execution_profile_id)
        .bind(node.execution_profile_version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_workflows(&self, limit: u32) -> Result<Vec<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            r#"SELECT workflow_id, title, cwd, state, current_revision, failure_message, created_at,
                      updated_at, started_at, completed_at
               FROM workflows
               ORDER BY created_at DESC, workflow_id DESC
               LIMIT ?"#,
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_workflows_requiring_convergence(&self) -> Result<Vec<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            r#"SELECT workflow_id, title, cwd, state, current_revision, failure_message, created_at,
                      updated_at, started_at, completed_at
               FROM workflows
               WHERE state IN ('running', 'paused', 'replanning', 'blocked')
               ORDER BY created_at, workflow_id"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_workflow(&self, workflow_id: &str) -> Result<Option<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            r#"SELECT workflow_id, title, cwd, state, current_revision, failure_message, created_at,
                      updated_at, started_at, completed_at
               FROM workflows WHERE workflow_id = ?"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_nodes(&self, workflow_id: &str) -> Result<Vec<WorkflowNodeRow>> {
        let Some(workflow) = self.get_workflow(workflow_id).await? else {
            return Ok(Vec::new());
        };
        self.list_nodes_at_revision(workflow_id, workflow.current_revision)
            .await
    }

    pub async fn list_nodes_at_revision(
        &self,
        workflow_id: &str,
        revision: i64,
    ) -> Result<Vec<WorkflowNodeRow>> {
        let Some(workflow) = self.get_workflow(workflow_id).await? else {
            return Ok(Vec::new());
        };
        if revision < 1 || revision > workflow.current_revision {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} does not have revision {revision}"
            )));
        }
        Ok(sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version,
                      introduced_revision, retired_revision, session_id, submitted_at,
                      submitted_runtime_instance_id, exit_request_started_at, created_at
               FROM workflow_nodes
               WHERE workflow_id = ?
                 AND introduced_revision <= ?
                 AND (retired_revision IS NULL OR retired_revision > ?)
               ORDER BY created_at, node_id"#,
        )
        .bind(workflow_id)
        .bind(revision)
        .bind(revision)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_node(&self, node_id: &str) -> Result<Option<WorkflowNodeRow>> {
        Ok(sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version,
                      introduced_revision, retired_revision, session_id, submitted_at,
                      submitted_runtime_instance_id, exit_request_started_at, created_at
               FROM workflow_nodes WHERE node_id = ?"#,
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_node_by_session(&self, session_id: &str) -> Result<Option<WorkflowNodeRow>> {
        let nodes = sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version,
                      introduced_revision, retired_revision, session_id, submitted_at,
                      submitted_runtime_instance_id, exit_request_started_at, created_at
               FROM workflow_nodes WHERE session_id = ?
               ORDER BY created_at, node_id
               LIMIT 2"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        match nodes.as_slice() {
            [] => Ok(None),
            [node] => Ok(Some(node.clone())),
            _ => Err(Error::StateConflict(format!(
                "session {session_id} is bound to multiple workflow nodes"
            ))),
        }
    }

    pub async fn bind_node_session(&self, node_id: &str, session_id: &str) -> Result<()> {
        let result = sqlx::query(
            r#"UPDATE workflow_nodes
               SET session_id = ?
               WHERE node_id = ?
                 AND session_id IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflows
                     WHERE workflows.workflow_id = workflow_nodes.workflow_id
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(session_id)
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} is missing or already has a session"
            )));
        }
        Ok(())
    }

    pub async fn claim_node_activation(&self, workflow_id: &str, node_id: &str) -> Result<()> {
        let result = sqlx::query(
            r#"UPDATE workflows
               SET activating_node_id = ?
               WHERE workflow_id = ?
                 AND state = 'running'
                 AND activating_node_id IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflow_nodes
                     WHERE workflow_nodes.workflow_id = workflows.workflow_id
                       AND workflow_nodes.node_id = ?
                       AND workflow_nodes.session_id IS NULL
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(node_id)
        .bind(workflow_id)
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} must be running without another node activation"
            )));
        }
        Ok(())
    }

    pub async fn finish_node_activation(&self, node_id: &str, session_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let node_result = sqlx::query(
            r#"UPDATE workflow_nodes
               SET session_id = ?
               WHERE node_id = ?
                 AND session_id IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflows
                     WHERE workflows.workflow_id = workflow_nodes.workflow_id
                       AND workflows.state = 'running'
                       AND workflows.activating_node_id = workflow_nodes.node_id
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(session_id)
        .bind(node_id)
        .execute(&mut *tx)
        .await?;
        if node_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} is not the claimed running activation"
            )));
        }
        let workflow_result = sqlx::query(
            r#"UPDATE workflows
               SET activating_node_id = NULL
               WHERE activating_node_id = ? AND state = 'running'"#,
        )
        .bind(node_id)
        .execute(&mut *tx)
        .await?;
        if workflow_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} activation claim disappeared"
            )));
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn release_node_activation(&self, workflow_id: &str, node_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE workflows SET activating_node_id = NULL WHERE workflow_id = ? AND activating_node_id = ?",
        )
        .bind(workflow_id)
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_node_submission(
        &self,
        node_id: &str,
        runtime_instance_id: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            r#"UPDATE workflow_nodes
               SET submitted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   submitted_runtime_instance_id = ?
               WHERE node_id = ?
                 AND submitted_at IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflows
                     WHERE workflows.workflow_id = workflow_nodes.workflow_id
                       AND workflows.state = 'running'
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(runtime_instance_id)
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} must be unsubmitted in a running workflow"
            )));
        }
        Ok(())
    }

    pub async fn claim_node_exit_request(
        &self,
        node_id: &str,
        runtime_instance_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"UPDATE workflow_nodes
               SET exit_request_started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE node_id = ?
                 AND submitted_runtime_instance_id = ?
                 AND exit_request_started_at IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflows
                     WHERE workflows.workflow_id = workflow_nodes.workflow_id
                       AND workflows.state = 'running'
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(node_id)
        .bind(runtime_instance_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn terminal_event_precedes_latest_resume(
        &self,
        workflow_id: &str,
        event_id: &str,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"SELECT EXISTS (
                   SELECT 1
                   FROM events AS agent_event
                   JOIN workflow_events AS resumed
                     ON resumed.workflow_id = ?
                    AND resumed.event_type = 'workflow.resumed'
                   WHERE agent_event.event_id = ?
                     AND resumed.sequence = (
                         SELECT MAX(sequence)
                         FROM workflow_events
                         WHERE workflow_id = ? AND event_type = 'workflow.resumed'
                     )
                     AND agent_event.created_at <= resumed.created_at
                     AND EXISTS (
                         SELECT 1
                         FROM workflow_events AS paused
                         WHERE paused.workflow_id = resumed.workflow_id
                           AND paused.event_type = 'workflow.paused'
                           AND paused.sequence < resumed.sequence
                     )
               )"#,
        )
        .bind(workflow_id)
        .bind(event_id)
        .bind(workflow_id)
        .fetch_one(&self.pool)
        .await?
            != 0)
    }

    pub async fn append_event(
        &self,
        event_id: &str,
        workflow_id: &str,
        event_type: &str,
        payload: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(event_type)
        .bind(payload)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_events(&self, workflow_id: &str) -> Result<Vec<WorkflowEventRow>> {
        Ok(sqlx::query_as::<_, WorkflowEventRow>(
            r#"SELECT event_id, workflow_id, sequence, event_type, payload, created_at
               FROM workflow_events WHERE workflow_id = ? ORDER BY sequence"#,
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn start_workflow(&self, workflow_id: &str, event_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE workflows
               SET state = 'running',
                   started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'pending'"#,
        )
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} must exist in pending state"
            )));
        }
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, 1, 'workflow.started', '{}')"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn pause_workflow(&self, workflow_id: &str, event_id: &str) -> Result<()> {
        self.transition_workflow_state(
            workflow_id,
            event_id,
            "running",
            "paused",
            "workflow.paused",
        )
        .await
    }

    pub async fn resume_workflow(&self, workflow_id: &str, event_id: &str) -> Result<()> {
        self.transition_workflow_state(
            workflow_id,
            event_id,
            "paused",
            "running",
            "workflow.resumed",
        )
        .await
    }

    async fn transition_workflow_state(
        &self,
        workflow_id: &str,
        event_id: &str,
        expected_state: &str,
        state: &str,
        event_type: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE workflows
               SET state = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ?
                 AND state = ?
                 AND activating_node_id IS NULL"#,
        )
        .bind(state)
        .bind(workflow_id)
        .bind(expected_state)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} must be {expected_state} without a node activation"
            )));
        }
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, ?, '{}')"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(event_type)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn complete_workflow(&self, workflow_id: &str, event_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE workflows
               SET state = 'completed',
                   completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'running'"#,
        )
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} must exist in running state"
            )));
        }
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.completed', '{}')"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .bind(sequence)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn idle_unsubmitted_workflow_node(
        &self,
        workflow_id: &str,
        node_id: &str,
        event_id: &str,
    ) -> Result<()> {
        self.transition_running_workflow(RunningWorkflowTransition {
            workflow_id,
            unsubmitted_node_id: Some(node_id),
            event_id,
            state: "idle",
            event_type: "workflow.idle",
            failure_message: None,
            payload: "{}",
        })
        .await
    }

    pub async fn fail_workflow(
        &self,
        workflow_id: &str,
        event_id: &str,
        failure_message: &str,
    ) -> Result<()> {
        let payload = serde_json::json!({ "failure_message": failure_message }).to_string();
        self.transition_running_workflow(RunningWorkflowTransition {
            workflow_id,
            unsubmitted_node_id: None,
            event_id,
            state: "failed",
            event_type: "workflow.failed",
            failure_message: Some(failure_message),
            payload: &payload,
        })
        .await
    }

    pub async fn fail_unsubmitted_workflow_node(
        &self,
        workflow_id: &str,
        node_id: &str,
        event_id: &str,
        failure_message: &str,
    ) -> Result<()> {
        let payload = serde_json::json!({ "failure_message": failure_message }).to_string();
        self.transition_running_workflow(RunningWorkflowTransition {
            workflow_id,
            unsubmitted_node_id: Some(node_id),
            event_id,
            state: "failed",
            event_type: "workflow.failed",
            failure_message: Some(failure_message),
            payload: &payload,
        })
        .await
    }

    pub async fn request_patch(
        &self,
        request: RequestWorkflowPatchRecord,
    ) -> Result<WorkflowPatchRow> {
        let mut tx = self.pool.begin().await?;
        crate::repositories::turns::SqliteTurnRepository::serialize_session_turn_writes_in_tx(
            &mut tx,
            &request.session_id,
        )
        .await?;

        let context = sqlx::query_as::<_, (String, String, String, i64)>(
            r#"SELECT n.workflow_id, n.node_id, t.turn_id, w.current_revision
               FROM workflow_nodes AS n
               JOIN workflows AS w ON w.workflow_id = n.workflow_id
               JOIN runtime_bindings AS r ON r.session_id = n.session_id
               JOIN turns AS t ON t.session_id = n.session_id
               WHERE n.session_id = ?
                 AND n.submitted_at IS NULL
                 AND n.introduced_revision <= w.current_revision
                 AND (n.retired_revision IS NULL OR n.retired_revision > w.current_revision)
                 AND w.state = 'running'
                 AND w.active_patch_id IS NULL
                 AND w.activating_node_id IS NULL
                 AND r.binding_state = 'confirmed'
                 AND r.runtime_instance_id = ?
                 AND t.state IN ('queued', 'running')
                 AND NOT EXISTS (
                     SELECT 1 FROM workflow_nodes AS child
                     WHERE child.workflow_id = n.workflow_id
                       AND child.parent_node_id = n.node_id
                       AND child.session_id IS NOT NULL
                       AND child.introduced_revision <= w.current_revision
                       AND (child.retired_revision IS NULL
                            OR child.retired_revision > w.current_revision)
                 )
               ORDER BY t.turn_id
               LIMIT 2"#,
        )
        .bind(&request.session_id)
        .bind(&request.runtime_instance_id)
        .fetch_all(&mut *tx)
        .await?;
        let [(workflow_id, node_id, turn_id, base_revision)] = context.as_slice() else {
            return Err(Error::StateConflict(format!(
                "session {} is not the current unsubmitted Agent Node of a running Workflow with the supplied Runtime and one active Turn",
                request.session_id
            )));
        };

        sqlx::query(
            r#"INSERT INTO workflow_patches
               (patch_id, workflow_id, requesting_node_id, requesting_session_id,
                requesting_turn_id, requesting_runtime_instance_id, replanner_creation_token,
                base_revision, state, request_document_ref, request_size_bytes)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'requested', ?, ?)"#,
        )
        .bind(&request.patch_id)
        .bind(workflow_id)
        .bind(node_id)
        .bind(&request.session_id)
        .bind(turn_id)
        .bind(&request.runtime_instance_id)
        .bind(&request.replanner_creation_token)
        .bind(base_revision)
        .bind(&request.request_document_ref)
        .bind(request.request_size_bytes)
        .execute(&mut *tx)
        .await?;

        let workflow_result = sqlx::query(
            r#"UPDATE workflows
               SET state = 'replanning', active_patch_id = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'running'
                 AND active_patch_id IS NULL AND activating_node_id IS NULL"#,
        )
        .bind(&request.patch_id)
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
        if workflow_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} cannot accept a Patch request"
            )));
        }

        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        let payload = serde_json::json!({
            "patch_id": &request.patch_id,
            "base_revision": base_revision,
            "requesting_node_id": node_id,
            "requesting_session_id": &request.session_id,
            "requesting_turn_id": turn_id,
            "request_document_ref": &request.request_document_ref,
            "request_size_bytes": request.request_size_bytes,
        });
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.patch_requested', ?)"#,
        )
        .bind(&request.event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(payload.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get_patch(&request.patch_id).await?.ok_or_else(|| {
            Error::Domain(format!(
                "accepted Workflow Patch {} is missing",
                request.patch_id
            ))
        })
    }

    pub async fn get_patch(&self, patch_id: &str) -> Result<Option<WorkflowPatchRow>> {
        Ok(sqlx::query_as::<_, WorkflowPatchRow>(
            r#"SELECT patch_id, workflow_id, requesting_node_id, requesting_session_id,
                      requesting_turn_id, requesting_runtime_instance_id, replanner_creation_token,
                      replanner_session_id, replanner_turn_id, replanner_runtime_instance_id,
                      base_revision, result_revision, state, request_document_ref,
                      request_size_bytes, decision_document_ref, reason_document_ref,
                      blocked_draft_ref, interruption_attempted_at, interruption_requested_at,
                      replanning_unlocked_at, continuation_message_id, continuation_queued_at,
                      replanner_exit_requested_at, requested_at, planning_at, resolved_at
               FROM workflow_patches WHERE patch_id = ?"#,
        )
        .bind(patch_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_active_patch_for_replanner(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<Option<WorkflowPatchRow>> {
        let patch_id: Option<String> = sqlx::query_scalar(
            r#"SELECT p.patch_id FROM workflow_patches AS p
               JOIN workflows AS w ON w.active_patch_id = p.patch_id
               JOIN sessions AS s ON s.session_id = p.replanner_session_id
               JOIN turns AS t ON t.turn_id = s.current_turn_id AND t.session_id = s.session_id
               JOIN runtime_bindings AS r ON r.session_id = s.session_id
               WHERE p.replanner_session_id = ? AND p.replanner_runtime_instance_id = ?
                 AND p.state = 'planning' AND w.state = 'replanning'
                 AND w.active_replanner_session_id = p.replanner_session_id
                 AND r.binding_state = 'confirmed' AND r.runtime_instance_id = ?
                 AND t.state IN ('queued', 'running')"#,
        )
        .bind(session_id)
        .bind(runtime_instance_id)
        .bind(runtime_instance_id)
        .fetch_optional(&self.pool)
        .await?;
        match patch_id {
            Some(patch_id) => self.get_patch(&patch_id).await,
            None => Ok(None),
        }
    }

    pub async fn get_active_patch(&self, workflow_id: &str) -> Result<Option<WorkflowPatchRow>> {
        Ok(sqlx::query_as::<_, WorkflowPatchRow>(
            r#"SELECT p.patch_id, p.workflow_id, p.requesting_node_id, p.requesting_session_id,
                      p.requesting_turn_id, p.requesting_runtime_instance_id,
                      p.replanner_creation_token, p.replanner_session_id, p.replanner_turn_id,
                      p.replanner_runtime_instance_id, p.base_revision, p.result_revision,
                      p.state, p.request_document_ref, p.request_size_bytes,
                      p.decision_document_ref, p.reason_document_ref, p.blocked_draft_ref,
                      p.interruption_attempted_at, p.interruption_requested_at,
                      p.replanning_unlocked_at, p.continuation_message_id,
                      p.continuation_queued_at, p.replanner_exit_requested_at,
                      p.requested_at, p.planning_at, p.resolved_at
               FROM workflows AS w
               JOIN workflow_patches AS p ON p.patch_id = w.active_patch_id
               WHERE w.workflow_id = ? AND w.state = 'replanning'
                 AND p.state IN ('requested', 'planning')"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn patch_requester_interrupted(&self, patch_id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"UPDATE workflow_patches
               SET replanning_unlocked_at = COALESCE(
                       replanning_unlocked_at,
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   ),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'requested'
                 AND EXISTS (
                     SELECT 1 FROM events
                     WHERE events.session_id = workflow_patches.requesting_session_id
                       AND events.turn_id = workflow_patches.requesting_turn_id
                       AND events.event_type = 'turn.interrupted'
                       AND events.source IN ('agent_adapter', 'agent_client')
                       AND json_extract(events.payload, '$.runtime_instance_id') =
                           workflow_patches.requesting_runtime_instance_id
                 )"#,
        )
        .bind(patch_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 1 {
            return Ok(true);
        }
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workflow_patches WHERE patch_id = ? AND replanning_unlocked_at IS NOT NULL",
        )
        .bind(patch_id)
        .fetch_one(&self.pool)
        .await? == 1)
    }

    pub async fn bind_patch_replanner(
        &self,
        patch_id: &str,
        session_id: &str,
        event_id: &str,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let runtime_instance_id: Option<String> = sqlx::query_scalar(
            r#"SELECT runtime_instance_id FROM runtime_bindings
               WHERE session_id = ? AND binding_state = 'confirmed'
                 AND runtime_instance_id IS NOT NULL"#,
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?
        .flatten();
        let Some(runtime_instance_id) = runtime_instance_id else {
            return Ok(false);
        };
        let result = sqlx::query(
            r#"UPDATE workflow_patches
               SET state = 'planning', replanner_session_id = ?,
                   replanner_runtime_instance_id = ?,
                   planning_at = COALESCE(planning_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'requested'
                 AND replanning_unlocked_at IS NOT NULL
                 AND replanner_session_id IS NULL"#,
        )
        .bind(session_id)
        .bind(&runtime_instance_id)
        .bind(patch_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        let workflow_id: String =
            sqlx::query_scalar("SELECT workflow_id FROM workflow_patches WHERE patch_id = ?")
                .bind(patch_id)
                .fetch_one(&mut *tx)
                .await?;
        let updated = sqlx::query(
            r#"UPDATE workflows SET active_replanner_session_id = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'replanning' AND active_patch_id = ?
                 AND active_replanner_session_id IS NULL"#,
        )
        .bind(session_id)
        .bind(&workflow_id)
        .bind(patch_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} cannot bind Re-planner Session {session_id}"
            )));
        }
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(&workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        let payload = serde_json::json!({
            "patch_id": patch_id,
            "replanner_session_id": session_id,
            "replanner_runtime_instance_id": runtime_instance_id,
        });
        sqlx::query(
            r#"INSERT INTO workflow_events (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.replanner_started', ?)"#,
        )
        .bind(event_id)
        .bind(&workflow_id)
        .bind(sequence)
        .bind(payload.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn block_patch(&self, request: BlockWorkflowPatchRecord) -> Result<WorkflowPatchRow> {
        let mut tx = self.pool.begin().await?;
        crate::repositories::turns::SqliteTurnRepository::serialize_session_turn_writes_in_tx(
            &mut tx,
            &request.session_id,
        )
        .await?;
        let context = sqlx::query_as::<_, (String, String, String, String)>(
            r#"SELECT p.patch_id, p.workflow_id, t.turn_id, r.runtime_instance_id
               FROM workflow_patches AS p
               JOIN workflows AS w ON w.active_patch_id = p.patch_id
               JOIN sessions AS s ON s.session_id = p.replanner_session_id
               JOIN turns AS t ON t.turn_id = s.current_turn_id AND t.session_id = s.session_id
               JOIN runtime_bindings AS r ON r.session_id = s.session_id
               WHERE p.state = 'planning' AND w.state = 'replanning'
                 AND w.active_replanner_session_id = ?
                 AND p.replanner_session_id = ?
                 AND p.replanner_runtime_instance_id = ?
                 AND r.binding_state = 'confirmed' AND r.runtime_instance_id = ?
                 AND t.state IN ('queued', 'running')"#,
        )
        .bind(&request.session_id)
        .bind(&request.session_id)
        .bind(&request.runtime_instance_id)
        .bind(&request.runtime_instance_id)
        .fetch_all(&mut *tx)
        .await?;
        let [(patch_id, workflow_id, turn_id, _)] = context.as_slice() else {
            return Err(Error::StateConflict(format!(
                "session {} is not the active Re-planner with one active Turn and the supplied Runtime",
                request.session_id
            )));
        };
        let patch_result = sqlx::query(
            r#"UPDATE workflow_patches
               SET state = 'blocked', replanner_turn_id = ?, reason_document_ref = ?,
                   blocked_draft_ref = ?, resolved_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'planning' AND replanner_session_id = ?
                 AND replanner_runtime_instance_id = ?"#,
        )
        .bind(turn_id)
        .bind(&request.reason_document_ref)
        .bind(&request.blocked_draft_ref)
        .bind(patch_id)
        .bind(&request.session_id)
        .bind(&request.runtime_instance_id)
        .execute(&mut *tx)
        .await?;
        if patch_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "Workflow Patch {patch_id} is already resolved"
            )));
        }
        let workflow_result = sqlx::query(
            r#"UPDATE workflows
               SET state = 'blocked', active_patch_id = NULL,
                   active_replanner_session_id = NULL,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'replanning' AND active_patch_id = ?
                 AND active_replanner_session_id = ?"#,
        )
        .bind(workflow_id)
        .bind(patch_id)
        .bind(&request.session_id)
        .execute(&mut *tx)
        .await?;
        if workflow_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} cannot be blocked"
            )));
        }
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        let payload = serde_json::json!({
            "patch_id": patch_id,
            "base_revision": sqlx::query_scalar::<_, i64>("SELECT base_revision FROM workflow_patches WHERE patch_id = ?").bind(patch_id).fetch_one(&mut *tx).await?,
            "outcome": "blocked",
            "reason_document_ref": &request.reason_document_ref,
            "blocked_draft_ref": &request.blocked_draft_ref,
            "replanner_session_id": &request.session_id,
            "replanner_turn_id": turn_id,
        });
        sqlx::query(
            r#"INSERT INTO workflow_events (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.patch_blocked', ?)"#,
        )
        .bind(&request.event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(payload.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get_patch(patch_id)
            .await?
            .ok_or_else(|| Error::Domain(format!("blocked Workflow Patch {patch_id} is missing")))
    }

    pub async fn get_blocked_patch_for_replanner(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowPatchRow>> {
        let patch_id: Option<String> = sqlx::query_scalar(
            r#"SELECT patch_id FROM workflow_patches
               WHERE workflow_id = ? AND state = 'blocked' AND replanner_session_id IS NOT NULL
               ORDER BY resolved_at DESC, patch_id DESC LIMIT 1"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?;
        match patch_id {
            Some(patch_id) => self.get_patch(&patch_id).await,
            None => Ok(None),
        }
    }

    pub async fn claim_patch_replanner_exit(&self, patch_id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"UPDATE workflow_patches SET replanner_exit_requested_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'blocked' AND replanner_exit_requested_at IS NULL"#,
        )
        .bind(patch_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn release_patch_replanner_exit(&self, patch_id: &str) -> Result<()> {
        sqlx::query("UPDATE workflow_patches SET replanner_exit_requested_at = NULL WHERE patch_id = ? AND state = 'blocked'")
            .bind(patch_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_patch_interruption_attempted(&self, patch_id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"UPDATE workflow_patches
               SET interruption_attempted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'requested'
                 AND replanning_unlocked_at IS NULL
                 AND interruption_requested_at IS NULL
                 AND (interruption_attempted_at IS NULL
                      OR interruption_attempted_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-2 seconds'))"#,
        )
        .bind(patch_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn mark_patch_interruption_requested(&self, patch_id: &str) -> Result<()> {
        sqlx::query(
            r#"UPDATE workflow_patches
               SET interruption_requested_at = COALESCE(
                       interruption_requested_at,
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   ),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'requested'"#,
        )
        .bind(patch_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn transition_running_workflow(
        &self,
        transition: RunningWorkflowTransition<'_>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE workflows
               SET state = ?,
                   failure_message = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ?
                 AND state = 'running'
                 AND (
                     ? IS NULL
                     OR EXISTS (
                         SELECT 1 FROM workflow_nodes
                         WHERE workflow_nodes.workflow_id = workflows.workflow_id
                           AND workflow_nodes.node_id = ?
                           AND workflow_nodes.submitted_at IS NULL
                           AND workflow_nodes.introduced_revision <= workflows.current_revision
                           AND (workflow_nodes.retired_revision IS NULL
                                OR workflow_nodes.retired_revision > workflows.current_revision)
                     )
                 )"#,
        )
        .bind(transition.state)
        .bind(transition.failure_message)
        .bind(transition.workflow_id)
        .bind(transition.unsubmitted_node_id)
        .bind(transition.unsubmitted_node_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            let expected = transition.unsubmitted_node_id.map_or_else(
                || "running state".to_string(),
                |node_id| format!("running state with unsubmitted node {node_id}"),
            );
            return Err(Error::StateConflict(format!(
                "workflow {} must exist in {expected}",
                transition.workflow_id
            )));
        }
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(transition.workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(transition.event_id)
        .bind(transition.workflow_id)
        .bind(sequence)
        .bind(transition.event_type)
        .bind(transition.payload)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
