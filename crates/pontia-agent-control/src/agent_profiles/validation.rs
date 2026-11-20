use pontia_core::error::{Error, Result};

use super::UpsertExecutionProfileRequest;

pub(super) fn validate_request(
    clients: &crate::clients::ClientRegistry,
    request: &UpsertExecutionProfileRequest,
) -> Result<()> {
    validate_non_empty("profile_id", &request.profile_id)?;
    validate_non_empty("version", &request.version)?;
    validate_non_empty("name", &request.name)?;
    match (
        &request.default_client_type,
        &request.default_client_key,
        &request.default_model,
    ) {
        (None, None, None) => {}
        (Some(client_type), Some(client_key), Some(model)) => {
            validate_non_empty("default_client_type", client_type)?;
            validate_non_empty("default_client_key", client_key)?;
            validate_non_empty("default_model", model)?;
            if !request.supported_client_types.contains(client_type) {
                return Err(Error::Domain(
                    "default client must be included in supported_client_types".into(),
                ));
            }
        }
        _ => {
            return Err(Error::Domain(
                "default_client_type, default_client_key and default_model must be set together"
                    .into(),
            ));
        }
    }
    for client_type in &request.supported_client_types {
        if clients.spec(client_type).is_none() {
            return Err(Error::Domain(format!(
                "unsupported client_type in supported_client_types: {client_type}"
            )));
        }
        if let Some(policy) = clients
            .get(client_type)
            .and_then(|client| client.instructions.as_ref())
        {
            policy.validate_instructions(request.system_prompt_template.as_deref())?;
        }
    }
    if request.agent_kind != "executor" {
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
