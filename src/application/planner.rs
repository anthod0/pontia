use std::{future::Future, path::PathBuf, pin::Pin, time::Duration};

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

#[derive(Debug, Clone)]
pub struct PiTaskPlanner {
    timeout: Duration,
    runtime: GenericRuntimeManager,
}

impl PiTaskPlanner {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            runtime: GenericRuntimeManager,
        }
    }

    pub async fn plan(&self, input: PlannerInput) -> Result<PlannerDecision> {
        <Self as TaskPlanner>::plan(self, input).await
    }
}

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

impl TaskPlanner for PiTaskPlanner {
    fn plan<'a>(
        &'a self,
        input: PlannerInput,
    ) -> Pin<Box<dyn Future<Output = Result<PlannerDecision>> + Send + 'a>> {
        Box::pin(async move { self.run_one_shot(input).await })
    }
}

impl PiTaskPlanner {
    async fn run_one_shot(&self, input: PlannerInput) -> Result<PlannerDecision> {
        let session_id = new_session_id().to_string();
        let turn_id = new_turn_id().to_string();
        let workspace = std::env::temp_dir()
            .join("llmparty-planner-workspaces")
            .join(&session_id);
        std::fs::create_dir_all(&workspace)?;

        let runtime = self.runtime.start_session(RuntimeStartRequest {
            session_id: session_id.clone(),
            client_type: "pi".to_string(),
            workspace: Some(workspace.display().to_string()),
            handle: None,
            role: Some("planner".to_string()),
        })?;
        let runtime_ref = runtime.runtime_ref.clone();
        let result = async {
            let prompt = build_pi_planner_prompt(&input);
            write_planner_current_turn_context(
                &runtime.metadata,
                &session_id,
                &turn_id,
                &input,
                &prompt,
            )?;
            self.runtime.dispatch_pi_turn(
                &runtime_ref,
                &AgentInput {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    input: prompt,
                },
            )?;
            let output = wait_for_planner_output(&runtime.metadata, &turn_id, self.timeout).await?;
            let json = extract_json_object(&output).ok_or_else(|| {
                Error::Domain("pi planner output did not contain a JSON object".to_string())
            })?;
            serde_json::from_str::<PlannerDecision>(json).map_err(Into::into)
        }
        .await;
        let _ = self.runtime.terminate_session(&runtime_ref);
        result
    }
}

fn build_pi_planner_prompt(input: &PlannerInput) -> String {
    let context = json!({
        "task_id": input.task_id,
        "task_input": input.input,
        "metadata": input.metadata,
        "candidate_workspaces": input.candidate_workspaces,
        "prior_planner_decisions": input.prior_decisions,
        "user_planner_messages": input.user_messages,
        "decision_schema": {
            "status": "resolved | needs_input | failed",
            "workspace": {"workspace_id": "optional", "canonical_path": "absolute path or null", "confidence": 0.0, "reason": "why"},
            "needs_input": {"question": "required when status is needs_input", "suggested_candidates": []},
            "reason": "required when failed; useful otherwise",
            "evidence": []
        }
    });
    format!(
        "You are llmparty's workspace-resolution planner. Do not execute the user task. Your only goal is to resolve the workspace for this task, ask for necessary input, or fail recoverably. Return only valid JSON matching the provided schema; do not include markdown or commentary.\n\nPlanner context:\n{}",
        serde_json::to_string_pretty(&context).unwrap_or_else(|_| "{}".to_string())
    )
}

fn write_planner_current_turn_context(
    metadata: &Value,
    session_id: &str,
    turn_id: &str,
    input: &PlannerInput,
    prompt: &str,
) -> Result<()> {
    let Some(path) = metadata
        .get("current_turn_file")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_string(&json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "input": prompt,
            "client_type": "pi",
            "task_id": input.task_id,
            "internal_event_url": metadata.get("internal_event_url").cloned().unwrap_or(Value::Null)
        }))?,
    )?;
    Ok(())
}

async fn wait_for_planner_output(
    metadata: &Value,
    turn_id: &str,
    timeout: Duration,
) -> Result<String> {
    let adapter_event_log = metadata
        .get("adapter_event_log")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Domain("pi planner runtime missing adapter_event_log".to_string()))?;
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(output) = read_planner_output(adapter_event_log, turn_id)? {
            return Ok(output);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Domain(format!(
                "pi planner timed out after {} ms",
                timeout.as_millis()
            )));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn read_planner_output(path: &str, turn_id: &str) -> Result<Option<String>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut latest_output = None;
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let event: Value = match serde_json::from_str(line) {
            Ok(event) => event,
            Err(_) => continue,
        };
        if event
            .get("turn_id")
            .and_then(Value::as_str)
            .is_some_and(|event_turn_id| event_turn_id != turn_id)
        {
            continue;
        }
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(event_type, "turn.output" | "turn.completed") {
            continue;
        }
        if let Some(output) = planner_output_from_event(&event) {
            latest_output = Some(output);
        }
        if event_type == "turn.completed" && latest_output.is_some() {
            return Ok(latest_output);
        }
    }
    Ok(latest_output)
}

fn planner_output_from_event(event: &Value) -> Option<String> {
    let payload = event.get("payload")?;
    if let Some(decision) = payload.get("planner_decision") {
        return Some(decision.to_string());
    }
    let output = payload.get("output").unwrap_or(payload);
    if let Some(decision) = output.get("planner_decision") {
        return Some(decision.to_string());
    }
    output
        .get("summary")
        .or_else(|| output.get("text"))
        .or_else(|| output.get("content"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn extract_json_object(output: &str) -> Option<&str> {
    let bytes = output.as_bytes();
    let start = bytes.iter().position(|byte| *byte == b'{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, byte) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'\"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'\"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return output.get(start..=start + offset);
                }
            }
            _ => {}
        }
    }
    None
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
