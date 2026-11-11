use pontia_core::error::{Error, Result};

use super::RuntimeBindingUpsertRequest;

pub(super) fn validate_required(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Domain(format!("{field} is required")));
    }
    Ok(())
}

pub(super) fn is_fork_start(request: &RuntimeBindingUpsertRequest) -> bool {
    matches!(request.start_kind.as_deref().map(str::trim), Some("fork"))
}

pub(super) fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}
