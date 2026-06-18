use super::*;

pub(super) fn validate_request(request: &UpsertExecutionProfileRequest) -> Result<()> {
    validate_non_empty("profile_id", &request.profile_id)?;
    validate_non_empty("version", &request.version)?;
    validate_non_empty("name", &request.name)?;
    for client_type in &request.supported_client_types {
        if !is_supported_client_type(client_type) {
            return Err(Error::Domain(format!(
                "unsupported client_type in supported_client_types: {client_type}"
            )));
        }
    }
    if !matches!(request.agent_kind.as_str(), "planner" | "executor") {
        return Err(Error::Domain(format!(
            "unsupported agent_kind: {}",
            request.agent_kind
        )));
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Domain(format!("{field} cannot be empty")));
    }
    Ok(())
}

pub(super) fn is_unique_constraint(error: &Error) -> bool {
    matches!(
        error,
        Error::Database(sqlx::Error::Database(database_error))
            if database_error.code().as_deref() == Some("1555")
                || database_error.message().contains("UNIQUE constraint failed")
    )
}

pub(super) fn ensure_not_builtin(profile: &ExecutionProfileView) -> Result<()> {
    if profile
        .metadata
        .get("builtin")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(Error::StateConflict(format!(
            "builtin execution profile {} cannot be modified or deleted",
            profile.profile_id
        )));
    }
    Ok(())
}
