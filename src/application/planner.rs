use std::{future::Future, pin::Pin};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannerRuntimeConfig {
    pub enabled: bool,
    pub client_type: String,
    pub timeout_ms: u64,
    pub compatibility_direct_dispatch: bool,
}

impl Default for PlannerRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            client_type: "pi".to_string(),
            timeout_ms: 30_000,
            compatibility_direct_dispatch: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SubmitPlannerInputRequest {
    pub message: String,
    #[serde(default = "default_client_type")]
    pub client_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannerInput {
    pub task_id: String,
    pub input: String,
    pub metadata: Value,
    pub candidate_workspaces: Vec<WorkspaceView>,
    pub prior_decisions: Vec<Value>,
    pub user_messages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannerDecision {
    #[serde(default)]
    pub decision_id: Option<String>,
    pub status: PlannerDecisionStatus,
    #[serde(default)]
    pub workspace: Option<PlannerWorkspaceDecision>,
    #[serde(default)]
    pub needs_input: Option<PlannerNeedsInput>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub evidence: Vec<PlannerEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlannerDecisionStatus {
    Resolved,
    NeedsInput,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannerWorkspaceDecision {
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub canonical_path: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannerNeedsInput {
    pub question: String,
    #[serde(default)]
    pub suggested_candidates: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlannerEvidence {
    #[serde(default)]
    pub evidence_id: Option<String>,
    pub kind: String,
    #[serde(default, rename = "ref")]
    pub ref_value: Option<String>,
    pub summary: String,
}

pub trait TaskPlanner: Send + Sync {
    fn plan<'a>(
        &'a self,
        input: PlannerInput,
    ) -> Pin<Box<dyn Future<Output = Result<PlannerDecision>> + Send + 'a>>;
}

#[derive(Debug, Default, Clone)]
pub struct FakeTaskPlanner;

impl TaskPlanner for FakeTaskPlanner {
    fn plan<'a>(
        &'a self,
        input: PlannerInput,
    ) -> Pin<Box<dyn Future<Output = Result<PlannerDecision>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(decision) = input.metadata.get("planner_decision") {
                return serde_json::from_value(decision.clone()).map_err(Into::into);
            }

            if let Some(message) = input.user_messages.last() {
                let trimmed = message.trim();
                if Path::new(trimmed).is_absolute() {
                    return Ok(PlannerDecision {
                        decision_id: None,
                        status: PlannerDecisionStatus::Resolved,
                        workspace: Some(PlannerWorkspaceDecision {
                            workspace_id: None,
                            canonical_path: Some(trimmed.to_string()),
                            confidence: Some(1.0),
                            reason: Some("user supplied workspace path".to_string()),
                        }),
                        needs_input: None,
                        reason: Some("user supplied workspace path".to_string()),
                        evidence: vec![PlannerEvidence {
                            evidence_id: None,
                            kind: "user_input".to_string(),
                            ref_value: None,
                            summary: "planner input contained an absolute workspace path"
                                .to_string(),
                        }],
                    });
                }
            }

            Ok(PlannerDecision {
                decision_id: None,
                status: PlannerDecisionStatus::NeedsInput,
                workspace: None,
                needs_input: Some(PlannerNeedsInput {
                    question: "Which workspace should this task use?".to_string(),
                    suggested_candidates: input
                        .candidate_workspaces
                        .into_iter()
                        .map(|workspace| {
                            json!({
                                "workspace_id": workspace.workspace_id,
                                "canonical_path": workspace.canonical_path,
                                "reason": "known workspace candidate"
                            })
                        })
                        .collect(),
                }),
                reason: Some("workspace could not be inferred".to_string()),
                evidence: vec![],
            })
        })
    }
}

#[derive(Clone)]
pub struct TaskPlannerService<P> {
    pool: SqlitePool,
    planner: P,
}

impl<P: TaskPlanner> TaskPlannerService<P> {
    pub fn new(pool: SqlitePool, planner: P) -> Self {
        Self { pool, planner }
    }

    pub async fn plan(&self, input: PlannerInput) -> Result<PlannerDecision> {
        let mut decision = self.planner.plan(input).await?;
        normalize_decision(&mut decision)?;
        Ok(decision)
    }

    pub async fn build_input(
        &self,
        task_id: &str,
        task_input: String,
        metadata: Value,
        additional_user_message: Option<String>,
    ) -> Result<PlannerInput> {
        let candidate_workspaces = ExternalQueryService::new(self.pool.clone())
            .list_workspaces()
            .await?;
        let rows = sqlx::query(
            r#"SELECT event_type, payload FROM task_events
               WHERE task_id = ? AND event_type IN ('task.planning_resolved', 'task.planning_needs_input', 'task.planning_failed', 'task.planning_input_received')
               ORDER BY created_at, event_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        let mut prior_decisions = Vec::new();
        let mut user_messages = Vec::new();
        for row in rows {
            let event_type: String = row.try_get("event_type")?;
            let payload: String = row.try_get("payload")?;
            let payload: Value = serde_json::from_str(&payload)?;
            if event_type == "task.planning_input_received" {
                if let Some(message) = payload.get("message").and_then(Value::as_str) {
                    user_messages.push(message.to_string());
                }
            } else if let Some(decision) = payload.get("decision") {
                prior_decisions.push(decision.clone());
            }
        }
        if let Some(message) = additional_user_message {
            user_messages.push(message);
        }

        Ok(PlannerInput {
            task_id: task_id.to_string(),
            input: task_input,
            metadata,
            candidate_workspaces,
            prior_decisions,
            user_messages,
        })
    }
}

fn normalize_decision(decision: &mut PlannerDecision) -> Result<()> {
    if decision.decision_id.is_none() {
        decision.decision_id = Some(format!("dec_{}", uuid::Uuid::now_v7()));
    }
    for evidence in &mut decision.evidence {
        if evidence.evidence_id.is_none() {
            evidence.evidence_id = Some(format!("ev_{}", uuid::Uuid::now_v7()));
        }
    }
    if let Some(workspace) = &mut decision.workspace
        && let Some(confidence) = workspace.confidence
    {
        workspace.confidence = Some(confidence.clamp(0.0, 1.0));
    }

    match decision.status {
        PlannerDecisionStatus::Resolved => {
            let workspace = decision.workspace.as_ref().ok_or_else(|| {
                Error::Domain("resolved planner decision requires workspace".to_string())
            })?;
            let has_workspace_id = workspace
                .workspace_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
            let has_abs_path = workspace
                .canonical_path
                .as_deref()
                .is_some_and(|value| Path::new(value).is_absolute());
            if !has_workspace_id && !has_abs_path {
                return Err(Error::Domain(
                    "resolved planner decision requires workspace_id or absolute canonical_path"
                        .to_string(),
                ));
            }
        }
        PlannerDecisionStatus::NeedsInput => {
            let question = decision
                .needs_input
                .as_ref()
                .map(|needs_input| needs_input.question.trim())
                .unwrap_or_default();
            if question.is_empty() {
                return Err(Error::Domain(
                    "needs_input planner decision requires a question".to_string(),
                ));
            }
        }
        PlannerDecisionStatus::Failed => {
            if decision
                .reason
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(Error::Domain(
                    "failed planner decision requires a reason".to_string(),
                ));
            }
        }
    }

    Ok(())
}
