use super::*;

pub(super) fn render_planning_context(
    role: AgentPlanningRole,
    task: &TaskView,
    dag: &TaskDagView,
    open_signals: &[DagSignalRecord],
    relevant_proposals: &[DagProposal],
    execution_profiles: &[ExecutionProfileView],
) -> String {
    let mut lines = vec![
        "pontia context: planning".to_string(),
        format!("Role: {}", planning_role_text(&role)),
        String::new(),
        "Task:".to_string(),
        format!("- Goal: {}", task.input),
        format!("- State: {}", task.state),
    ];
    if let Some(workspace_id) = non_empty(task.workspace_id.as_deref()) {
        lines.push(format!("- Workspace: {workspace_id}"));
    }

    lines.push(String::new());
    if dag.work_items.is_empty() {
        lines.push("Current DAG: none yet.".to_string());
    } else {
        lines.push("Current DAG:".to_string());
        lines.push(format!(
            "- Summary: total {}, ready {}, running {}, completed {}, blocked {}, failed {}, open signals {}",
            dag.summary.total_work_items,
            dag.summary.ready_work_items,
            dag.summary.running_work_items,
            dag.summary.completed_work_items,
            dag.summary.blocked_work_items,
            dag.summary.failed_work_items,
            dag.summary.open_signals
        ));
        lines.push("- Work items:".to_string());
        for item in &dag.work_items {
            let state = item
                .runtime
                .as_ref()
                .map(|runtime| runtime.current_state.as_str())
                .unwrap_or("unknown");
            lines.push(format!(
                "  - {} [{}] {}",
                item.work_item.work_item_id, state, item.work_item.title
            ));
            push_optional(
                &mut lines,
                "    Description",
                non_empty(Some(&item.work_item.description)),
            );
            push_optional(
                &mut lines,
                "    Action",
                non_empty(Some(&item.work_item.action)),
            );
            lines.push(format!(
                "    Profile: {}",
                item.work_item.execution_profile_id
            ));
            let depends_on: Vec<_> = dag
                .edges
                .iter()
                .filter(|edge| edge.to_work_item_id == item.work_item.work_item_id)
                .map(|edge| edge.from_work_item_id.as_str())
                .collect();
            if !depends_on.is_empty() {
                lines.push(format!("    Depends on: {}", depends_on.join(", ")));
            }
            push_value_list(
                &mut lines,
                "    Acceptance",
                &item.work_item.acceptance_criteria,
                None,
            );
        }
    }

    lines.push(String::new());
    push_signals(&mut lines, "Open signals", open_signals);

    lines.push(String::new());
    if relevant_proposals.is_empty() {
        lines.push("Relevant proposals: none.".to_string());
    } else {
        lines.push("Relevant proposals:".to_string());
        for proposal in relevant_proposals {
            lines.push(format!(
                "- {} [{} / {}]: {}",
                proposal.proposal_id, proposal.state, proposal.mode, proposal.summary
            ));
        }
    }

    lines.push(String::new());
    if execution_profiles.is_empty() {
        lines.push("Available execution profiles: none.".to_string());
    } else {
        lines.push("Available execution profiles:".to_string());
        for profile in execution_profiles {
            let mut line = format!("- {}: {}", profile.profile_id, profile.name);
            if let Some(description) = non_empty(profile.description.as_deref()) {
                line.push_str(&format!(" — {description}"));
            }
            lines.push(line);
            if !profile.supported_client_types.is_empty() {
                lines.push(format!(
                    "  Clients: {}",
                    profile.supported_client_types.join(", ")
                ));
            }
            push_optional(
                &mut lines,
                "  Expected output",
                non_empty(profile.expected_output_schema.as_deref()),
            );
        }
    }

    lines.join("\n")
}

pub(super) fn render_execution_context(
    task: &TaskView,
    work_item: &WorkItemWithRuntimeView,
    work_item_run: &WorkItemRunRecord,
    upstream_completed_items: &[WorkItemWithRuntimeView],
    acceptance_criteria: &Value,
    open_signals: &[DagSignalRecord],
) -> String {
    let mut lines = vec![
        "pontia context: execution".to_string(),
        String::new(),
        "Task:".to_string(),
        format!("- Goal: {}", task.input),
        format!("- State: {}", task.state),
    ];
    if let Some(workspace_id) = non_empty(task.workspace_id.as_deref()) {
        lines.push(format!("- Workspace: {workspace_id}"));
    }

    lines.push(String::new());
    lines.push("Current WorkItem:".to_string());
    lines.push(format!("- ID: {}", work_item.work_item.work_item_id));
    lines.push(format!("- Title: {}", work_item.work_item.title));
    push_optional(
        &mut lines,
        "- Description",
        non_empty(Some(&work_item.work_item.description)),
    );
    push_optional(
        &mut lines,
        "- Action",
        non_empty(Some(&work_item.work_item.action)),
    );
    lines.push(format!(
        "- Profile: {}",
        work_item.work_item.execution_profile_id
    ));
    lines.push(format!("- Attempt: {}", work_item_run.attempt));
    lines.push(format!("- Run state: {}", work_item_run.state));
    push_value_list(
        &mut lines,
        "- Acceptance criteria",
        acceptance_criteria,
        Some("none specified."),
    );

    lines.push(String::new());
    if upstream_completed_items.is_empty() {
        lines.push("Completed dependencies: none.".to_string());
    } else {
        lines.push("Completed dependencies:".to_string());
        for item in upstream_completed_items {
            let state = item
                .runtime
                .as_ref()
                .map(|runtime| runtime.current_state.as_str())
                .unwrap_or("completed");
            lines.push(format!(
                "- {} [{}] {}",
                item.work_item.work_item_id, state, item.work_item.title
            ));
        }
    }

    lines.push(String::new());
    push_signals(&mut lines, "Open related signals", open_signals);

    lines.push(String::new());
    lines.push("Next:".to_string());
    lines.push("- Execute only this WorkItem.".to_string());
    lines.push("- Call submitResult when finished.".to_string());
    lines
        .push("- Call raiseSignal if blocked, missing input, or replanning is needed.".to_string());

    lines.join("\n")
}

fn planning_role_text(role: &AgentPlanningRole) -> &'static str {
    match role {
        AgentPlanningRole::Planner => "planner",
        AgentPlanningRole::Replanner => "replanner",
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn push_optional(lines: &mut Vec<String>, label: &str, value: Option<&str>) {
    if let Some(value) = value {
        lines.push(format!("{label}: {value}"));
    }
}

fn push_value_list(lines: &mut Vec<String>, label: &str, value: &Value, empty_text: Option<&str>) {
    let items = value
        .as_array()
        .map(|array| {
            array
                .iter()
                .filter_map(|item| item.as_str().and_then(|text| non_empty(Some(text))))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if items.is_empty() {
        if let Some(empty_text) = empty_text {
            lines.push(format!("{label}: {empty_text}"));
        }
        return;
    }

    lines.push(format!("{label}:"));
    for item in items {
        lines.push(format!("  - {item}"));
    }
}

fn push_signals(lines: &mut Vec<String>, label: &str, signals: &[DagSignalRecord]) {
    if signals.is_empty() {
        lines.push(format!("{label}: none."));
        return;
    }

    lines.push(format!("{label}:"));
    for signal in signals {
        lines.push(format!(
            "- {} [{} / {}]: {}",
            signal.signal_id, signal.severity, signal.kind, signal.summary
        ));
        push_optional(lines, "  Detail", non_empty(signal.detail.as_deref()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn planning_context_is_factual_and_has_no_workflow_guidance() {
        let task = TaskView {
            task_id: "task_1".to_string(),
            state: "planning".to_string(),
            input: "你好".to_string(),
            workspace_id: Some("workspace_1".to_string()),
            session_id: None,
            turn_id: None,
            routing_state: "none".to_string(),
            routing_reason: None,
            routing_confidence: None,
            metadata: json!({}),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let dag = TaskDagView {
            task_id: "task_1".to_string(),
            summary: DagSummaryView {
                total_work_items: 0,
                ready_work_items: 0,
                running_work_items: 0,
                completed_work_items: 0,
                blocked_work_items: 0,
                failed_work_items: 0,
                open_signals: 0,
                total_runs: 0,
            },
            work_items: Vec::new(),
            edges: Vec::new(),
            runs: Vec::new(),
            signals: Vec::new(),
        };

        let text = render_planning_context(AgentPlanningRole::Planner, &task, &dag, &[], &[], &[]);

        assert!(text.contains("pontia context: planning"));
        assert!(text.contains("Goal: 你好"));
        for disallowed in [
            "Next:",
            "Submit an initial DAG",
            "Submit a DAG patch",
            "submitPlan",
            "raiseSignal",
            "Do not include",
            "supersede_policy",
            "scheduler",
        ] {
            assert!(
                !text.contains(disallowed),
                "planning context should not contain workflow guidance: {disallowed}\n{text}"
            );
        }
    }
}
