use pontia_application::{agent_clients::AgentClientDetails, client_contract::ClientIntegration};
use pontia_runtime::local_service::{CommandRunner, DefinitionStore};
use std::{collections::HashMap, path::Path, sync::Arc};

/// Run after binary installation and service recovery have committed. Failures here
/// cannot roll back external plugin installations or the already-updated binaries.
pub fn maintain_enabled(
    entries: &[AgentClientDetails],
    integrations: &[Arc<dyn ClientIntegration>],
    vars: &HashMap<String, String>,
    home: &Path,
    runner: &dyn CommandRunner,
    definitions: &dyn DefinitionStore,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for entry in entries
        .iter()
        .filter(|entry| entry.has_record && entry.enabled)
    {
        // Integrations without local initialization artifacts have nothing to maintain.
        let Some(integration) = integrations.iter().find(|integration| {
            integration.client_type() == entry.client_type
                && integration.client_key() == entry.client_key
        }) else {
            continue;
        };
        if let Err(error) = integration.maintain(vars, home, runner, definitions) {
            failures.push(format!("{}: {error}", entry.name));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Pontia was updated, but client integration maintenance failed (external changes were not rolled back): {}",
            failures.join("; ")
        ))
    }
}
