use lbug::{LogicalType, Value as LbugValue};
use serde_json::Value;

use pontia_core::{
    error::{Error, Result},
    time::utc_now,
};

use super::GraphEdgeKind;

pub(super) fn rel_label(edge_type: GraphEdgeKind) -> &'static str {
    match edge_type {
        GraphEdgeKind::DependsOn => "DEPENDS_ON",
        GraphEdgeKind::Reviews => "REVIEWS",
        GraphEdgeKind::Supersedes => "SUPERSEDES",
        GraphEdgeKind::CausedBy => "CAUSED_BY",
    }
}
pub(super) fn string_value(value: impl Into<String>) -> LbugValue {
    LbugValue::String(value.into())
}

pub(super) fn optional_string_value(value: Option<String>) -> LbugValue {
    value.map_or(LbugValue::Null(LogicalType::String), LbugValue::String)
}

pub(super) fn optional_json_value(value: Option<Value>) -> Result<LbugValue> {
    value
        .map(json_value)
        .transpose()
        .map(|value| value.unwrap_or(LbugValue::Null(LogicalType::String)))
}

pub(super) fn json_value(value: Value) -> Result<LbugValue> {
    Ok(LbugValue::String(serde_json::to_string(&value)?))
}

pub(super) fn expect_string(value: &LbugValue) -> Result<String> {
    optional_string(value)?.ok_or_else(|| Error::Domain("expected lbug string".to_string()))
}

pub(super) fn optional_string(value: &LbugValue) -> Result<Option<String>> {
    match value {
        LbugValue::String(value) => Ok(Some(value.clone())),
        LbugValue::Null(_) => Ok(None),
        other => Err(Error::Domain(format!(
            "expected lbug string, got {other:?}"
        ))),
    }
}

pub(super) fn expect_i64(value: &LbugValue) -> Result<i64> {
    match value {
        LbugValue::Int64(value) => Ok(*value),
        other => Err(Error::Domain(format!("expected lbug int64, got {other:?}"))),
    }
}

pub(super) fn expect_bool(value: &LbugValue) -> Result<bool> {
    match value {
        LbugValue::Bool(value) => Ok(*value),
        other => Err(Error::Domain(format!("expected lbug bool, got {other:?}"))),
    }
}

pub(super) fn parse_json_value(value: &LbugValue) -> Result<Value> {
    let raw = expect_string(value)?;
    Ok(serde_json::from_str(&raw)?)
}

pub(super) fn optional_json(value: &LbugValue) -> Result<Option<Value>> {
    optional_string(value)?
        .map(|raw| serde_json::from_str(&raw))
        .transpose()
        .map_err(Into::into)
}

pub(super) fn now_string() -> String {
    utc_now()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}
