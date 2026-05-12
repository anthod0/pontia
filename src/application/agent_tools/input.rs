use super::*;

pub(super) fn default_agent_tool_input() -> Value {
    json!({})
}

pub(super) fn is_known_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "getContext" | "submitPlan" | "submitResult" | "raiseSignal"
    )
}

pub(super) fn validate_required(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(Error::Domain(format!("{field} is required")))
    } else {
        Ok(())
    }
}

pub(super) fn parse_planning_role(role: &str) -> Result<AgentPlanningRole> {
    match role {
        "planner" => Ok(AgentPlanningRole::Planner),
        "replanner" => Ok(AgentPlanningRole::Replanner),
        other => Err(Error::StateConflict(format!(
            "unsupported DAG planning role {other}"
        ))),
    }
}

pub(super) fn parse_submit_plan_initial_input(input: Value) -> Result<SubmitPlanPayload> {
    let mode = input
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("initial_dag");
    if mode != "initial_dag" {
        return Err(Error::Domain(format!(
            "submitPlan initial payload mode must be initial_dag, got {mode}"
        )));
    }
    let dag = input.get("dag").unwrap_or(&input);
    Ok(SubmitPlanPayload {
        mode: "initial_dag".to_string(),
        summary: required_input_string(&input, "summary")?,
        work_items: serde_json::from_value(
            dag.get("work_items").cloned().unwrap_or_else(|| json!([])),
        )?,
        edges: serde_json::from_value(dag.get("edges").cloned().unwrap_or_else(|| json!([])))?,
        assumptions: serde_json::from_value(
            input
                .get("assumptions")
                .cloned()
                .unwrap_or_else(|| json!([])),
        )?,
        risks: serde_json::from_value(input.get("risks").cloned().unwrap_or_else(|| json!([])))?,
    })
}

pub(super) fn parse_submit_plan_patch_input(input: Value) -> Result<(String, DagPatch)> {
    let mode = input.get("mode").and_then(Value::as_str).unwrap_or("patch");
    if mode != "patch" {
        return Err(Error::Domain(format!(
            "submitPlan patch payload mode must be patch, got {mode}"
        )));
    }
    let summary = required_input_string(&input, "summary")?;
    let mut patch_value = input.get("patch").cloned().unwrap_or_else(
        || json!({"operations": input.get("operations").cloned().unwrap_or_else(|| json!([]))}),
    );
    if patch_value.get("summary").is_none()
        && let Some(object) = patch_value.as_object_mut()
    {
        object.insert("summary".to_string(), Value::String(summary.clone()));
    }
    let mut patch: DagPatch = serde_json::from_value(patch_value)?;
    if patch.summary.is_empty() {
        patch.summary = summary.clone();
    }
    Ok((summary, patch))
}

fn required_input_string(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| Error::Domain(format!("submitPlan input missing string field {key}")))
}

pub(super) async fn reject_duplicate_successful_submit_plan(
    pool: &SqlitePool,
    context: &AgentToolContext,
) -> Result<()> {
    let existing: Option<String> = sqlx::query_scalar(
        r#"SELECT proposal_id FROM dag_proposals
           WHERE task_id = ? AND created_by_session_id = ? AND state = 'applied'
           ORDER BY created_at DESC, proposal_id DESC LIMIT 1"#,
    )
    .bind(&context.task_id)
    .bind(&context.session_id)
    .fetch_optional(pool)
    .await?;
    if let Some(proposal_id) = existing {
        Err(Error::StateConflict(format!(
            "submitPlan already applied proposal {proposal_id} for this planning session"
        )))
    } else {
        Ok(())
    }
}
