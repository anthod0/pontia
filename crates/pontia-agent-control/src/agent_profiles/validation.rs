use super::UpsertAgentProfileRequest;
use pontia_core::{Error, Result};

/// New Profile IDs are written by hand in `--profile-id` and Workflow definitions, so they
/// are limited to characters that need no quoting there. Existing IDs stay editable.
pub(super) fn validate_new_profile_id(id: &str) -> Result<()> {
    let mut chars = id.chars();
    if chars
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        return Ok(());
    }
    Err(Error::Domain(
        "profile_id must start with a lowercase letter or digit and contain only lowercase letters, digits, '.', '_' or '-'".into(),
    ))
}

pub(super) fn validate_request(
    clients: &crate::clients::ClientRegistry,
    request: &UpsertAgentProfileRequest,
) -> Result<()> {
    validate_non_empty("profile_id", &request.profile_id)?;
    validate_non_empty("name", &request.name)?;
    if let Some(effort) = &request.reasoning_effort {
        validate_non_empty("reasoning_effort", effort)?;
        if request.model.is_none() {
            return Err(Error::Domain("reasoning_effort requires a model".into()));
        }
    }
    match (&request.client_type, &request.client_key, &request.model) {
        (None, None, None) => {}
        (Some(client), Some(key), Some(model)) => {
            validate_non_empty("client_type", client)?;
            validate_non_empty("client_key", key)?;
            validate_non_empty("model", model)?;
        }
        _ => {
            return Err(Error::Domain(
                "client_type, client_key and model must be set together".into(),
            ));
        }
    }
    crate::sessions::instructions::validate_static_instructions(request.system_prompt.as_deref())?;
    if let Some(client) = &request.client_type {
        if let Some(policy) = clients
            .get(client)
            .and_then(|client| client.instructions.as_ref())
        {
            policy.validate_instructions(request.system_prompt.as_deref())?;
        } else if request.system_prompt.is_some() {
            return Err(Error::CapabilityUnavailable(
                "Client cannot inject system instructions".into(),
            ));
        }
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
