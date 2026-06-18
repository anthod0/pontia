use super::*;

pub(super) fn terminal_summary(payload: &Value) -> Option<String> {
    nested_string(payload, &["output", "summary"])
        .or_else(|| nested_string(payload, &["output_summary"]))
        .or_else(|| nested_string(payload, &["summary"]))
        .or_else(|| nested_string(payload, &["output", "text"]))
        .or_else(|| nested_string(payload, &["output", "content"]))
        .or_else(|| {
            payload
                .get("output")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

pub(super) fn failure_summary(payload: &Value) -> String {
    nested_string(payload, &["failure", "message"])
        .or_else(|| nested_string(payload, &["message"]))
        .unwrap_or_else(|| "turn failed".to_string())
}

pub(super) fn parsed_payload_to_result(payload: SubmitResultPayload) -> ParsedRunResult {
    ParsedRunResult {
        state: normalize_result_status(&payload.status),
        summary: payload.summary,
        outputs: payload.outputs,
        failure: payload.failure,
        signals: payload.signals,
    }
}

pub(super) fn validate_result_status(status: &str) -> Result<()> {
    match status {
        "completed" | "failed" | "blocked" | "needs_input" => Ok(()),
        other => Err(Error::Domain(format!(
            "submitResult status must be completed, failed, blocked, or needs_input, got {other}"
        ))),
    }
}

pub(super) fn normalize_result_status(status: &str) -> String {
    match status {
        "completed" | "failed" | "blocked" | "needs_input" => status.to_string(),
        _ => "completed".to_string(),
    }
}

pub(super) fn outcome_state_for_status(status: &str) -> &'static str {
    match status {
        "completed" => "succeeded",
        "failed" => "failed",
        "blocked" | "needs_input" => "blocked",
        _ => "unknown",
    }
}

pub(super) fn signal_blocking_state(kind: &str) -> &'static str {
    match kind {
        "needs_input" | "assistance_needed" => "needs_input",
        _ => "blocked",
    }
}

pub(super) fn signal_projection_state(kind: &str) -> &'static str {
    match kind {
        "replan_requested" => "replan_anchor",
        _ => signal_blocking_state(kind),
    }
}

pub(super) fn validate_signal_kind(kind: &str) -> Result<()> {
    match kind {
        "needs_input" | "replan_requested" | "risk" | "missing_dependency" | "scope_change"
        | "assistance_needed" | "review_requested" | "other" => Ok(()),
        other => Err(Error::Domain(format!(
            "raiseSignal kind is not supported: {other}"
        ))),
    }
}

pub(super) fn normalize_severity(severity: &str) -> &str {
    match severity {
        "low" | "medium" | "high" => severity,
        _ => "medium",
    }
}

pub(super) fn is_terminal_run_state(state: &str) -> bool {
    matches!(
        state,
        "completed" | "failed" | "blocked" | "needs_input" | "cancelled"
    )
}

pub(super) fn new_dag_run_result_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7())
}
