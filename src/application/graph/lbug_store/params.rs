use lbug::Value as LbugValue;

use crate::error::Result;

use super::values::{
    json_value, now_string, optional_json_value, optional_string_value, string_value,
};
use super::{AddWorkItemEdgeRequest, UpsertSignalRequest, UpsertWorkItemRequest};

pub(super) fn work_item_params(
    request: UpsertWorkItemRequest,
    updated_at: Option<String>,
) -> Result<Vec<(&'static str, LbugValue)>> {
    let mut params = vec![
        ("work_item_id", string_value(request.work_item_id)),
        ("task_id", string_value(request.task_id)),
        ("title", string_value(request.title)),
        ("description", string_value(request.description)),
        ("kind", string_value(request.kind)),
        ("action", string_value(request.action)),
        (
            "execution_profile_id",
            string_value(request.execution_profile_id),
        ),
        (
            "execution_profile_version",
            optional_string_value(request.execution_profile_version),
        ),
        ("review_policy", optional_json_value(request.review_policy)?),
        (
            "execution_policy",
            optional_json_value(request.execution_policy)?,
        ),
        (
            "escalation_policy",
            optional_json_value(request.escalation_policy)?,
        ),
        ("priority", LbugValue::Int64(request.priority)),
        ("optional_value", LbugValue::Bool(request.optional)),
        ("parallelizable", LbugValue::Bool(request.parallelizable)),
        (
            "acceptance_criteria",
            json_value(request.acceptance_criteria)?,
        ),
        ("active", LbugValue::Bool(request.active)),
        ("ref", optional_string_value(request.ref_)),
        ("metadata", json_value(request.metadata)?),
    ];
    if let Some(updated_at) = updated_at {
        params.push(("updated_at", string_value(updated_at)));
    }
    Ok(params)
}

pub(super) fn signal_params(
    request: UpsertSignalRequest,
    updated_at: Option<String>,
) -> Result<Vec<(&'static str, LbugValue)>> {
    let mut params = vec![
        ("signal_id", string_value(request.signal_id)),
        ("task_id", string_value(request.task_id)),
        ("work_item_id", optional_string_value(request.work_item_id)),
        ("run_id", optional_string_value(request.run_id)),
        (
            "source_session_id",
            optional_string_value(request.source_session_id),
        ),
        ("source", string_value(request.source)),
        ("kind", string_value(request.kind)),
        ("summary", string_value(request.summary)),
        ("detail", optional_string_value(request.detail)),
        ("severity", string_value(request.severity)),
        ("related_refs", json_value(request.related_refs)?),
        ("state", string_value(request.state)),
        ("ref", optional_string_value(request.ref_)),
        ("metadata", json_value(request.metadata)?),
    ];
    if let Some(updated_at) = updated_at {
        params.push(("updated_at", string_value(updated_at)));
    }
    Ok(params)
}

pub(super) fn edge_params(
    request: &AddWorkItemEdgeRequest,
    edge_id: Option<String>,
) -> Vec<(&'static str, LbugValue)> {
    let mut params = vec![
        ("task_id", string_value(&request.task_id)),
        (
            "from_work_item_id",
            string_value(&request.from_work_item_id),
        ),
        ("to_work_item_id", string_value(&request.to_work_item_id)),
        ("ref", optional_string_value(request.ref_.clone())),
    ];
    if let Some(edge_id) = edge_id {
        params.push(("edge_id", string_value(edge_id)));
        params.push(("metadata", string_value("{}")));
        params.push(("created_at", string_value(now_string())));
    }
    params
}
