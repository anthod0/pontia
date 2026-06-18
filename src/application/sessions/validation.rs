use super::*;
use crate::agent_clients::{DispatchMode, ReadinessMode, get_client_spec};

pub(super) fn client_dispatch_mode(client_type: &str) -> Result<DispatchMode> {
    get_client_spec(client_type)
        .map(|spec| spec.adapter.dispatch)
        .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))
}

pub(super) fn client_readiness_mode(client_type: &str) -> Result<ReadinessMode> {
    get_client_spec(client_type)
        .map(|spec| spec.adapter.readiness)
        .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))
}

pub(crate) fn pontia_agent_kind(metadata: &Value) -> Option<String> {
    if metadata.get("dag_managed").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    if metadata
        .get("dag_planning_role")
        .and_then(Value::as_str)
        .is_some()
    {
        return Some("planner".to_string());
    }
    if metadata
        .get("work_item_id")
        .and_then(Value::as_str)
        .is_some()
    {
        return Some("executor".to_string());
    }
    None
}

pub(super) fn validate_handle(handle: &str) -> Result<()> {
    let mut chars = handle.chars();
    if chars.next() != Some('@') {
        return Err(invalid_handle(handle));
    }
    let Some(first) = chars.next() else {
        return Err(invalid_handle(handle));
    };
    if !first.is_ascii_lowercase() {
        return Err(invalid_handle(handle));
    }
    let remaining: Vec<char> = chars.collect();
    if remaining.is_empty() || remaining.len() > 30 {
        return Err(invalid_handle(handle));
    }
    if !remaining
        .iter()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == '_' || *ch == '-')
    {
        return Err(invalid_handle(handle));
    }
    Ok(())
}

fn invalid_handle(handle: &str) -> Error {
    Error::Domain(format!(
        "Invalid session handle {handle}. Handle must match @[a-z][a-z0-9_-]{{1,31}}."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pontia_agent_kind_maps_planning_sessions_to_planner() {
        assert_eq!(
            pontia_agent_kind(&json!({
                "dag_managed": true,
                "dag_planning_role": "replanner",
                "task_id": "task_1"
            })),
            Some("planner".to_string())
        );
    }

    #[test]
    fn pontia_agent_kind_maps_work_item_sessions_to_executor() {
        assert_eq!(
            pontia_agent_kind(&json!({
                "dag_managed": true,
                "task_id": "task_1",
                "work_item_id": "wi_1"
            })),
            Some("executor".to_string())
        );
    }

    #[test]
    fn pontia_agent_kind_ignores_non_dag_sessions() {
        assert_eq!(pontia_agent_kind(&json!({ "work_item_id": "wi_1" })), None);
    }
}
