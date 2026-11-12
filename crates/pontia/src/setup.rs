use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use pontia_agent_control::client_contract::ClientIntegration;
use pontia_config::AppConfig;
use pontia_runtime::local_service::{CommandRunner, DefinitionStore};

use crate::{
    init::{interactive_agent_selection, line_agent_selection, validated_user_home},
    local_config,
};

pub trait SetupPlatform {
    fn integrations(&self) -> Vec<Arc<dyn ClientIntegration>>;
    fn command_runner(&self) -> &dyn CommandRunner;
    fn definition_store(&self) -> &dyn DefinitionStore;
    fn preflight(&self) -> Result<(), String>;
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), String>;
    fn start_service(
        &self,
        config: &AppConfig,
        config_changed: bool,
        environment_paths: &[(String, PathBuf)],
    ) -> Result<(), String>;
    fn persisted_environment_paths(
        &self,
        user_home: &Path,
    ) -> Result<Vec<(String, PathBuf)>, String>;
    fn enable_client(&self, config: &AppConfig, client: &str, key: &str) -> Result<(), String>;
}

pub(crate) fn integration_vars<P: SetupPlatform>(
    vars: &HashMap<String, String>,
    user_home: &Path,
    platform: &P,
) -> Result<HashMap<String, String>, String> {
    let mut vars = vars.clone();
    for (name, path) in platform.persisted_environment_paths(user_home)? {
        vars.entry(name)
            .or_insert_with(|| path.to_string_lossy().into_owned());
    }
    Ok(vars)
}

pub fn run<R: BufRead, W: Write, P: SetupPlatform>(
    client: Option<&str>,
    input: &mut R,
    output: &mut W,
    vars: &HashMap<String, String>,
    platform: &P,
) -> Result<(), String> {
    run_with_selector(client, input, output, vars, platform, line_agent_selection)
}

pub fn run_interactive<P: SetupPlatform>(
    client: Option<&str>,
    vars: &HashMap<String, String>,
    platform: &P,
) -> Result<(), String> {
    run_with_selector(
        client,
        &mut io::stdin().lock(),
        &mut io::stdout().lock(),
        vars,
        platform,
        interactive_agent_selection,
    )
}

fn run_with_selector<R, W, P, S>(
    client: Option<&str>,
    input: &mut R,
    output: &mut W,
    vars: &HashMap<String, String>,
    platform: &P,
    select: S,
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
    P: SetupPlatform,
    S: FnOnce(&mut R, &mut W, &[Arc<dyn ClientIntegration>]) -> Result<Vec<usize>, String>,
{
    let integrations = platform.integrations();
    let selected = match client {
        Some(name) => vec![
            integrations
                .iter()
                .position(|client| client.client_type() == name)
                .ok_or_else(|| format!("unsupported Agent Client: {name}"))?,
        ],
        None => select(input, output, &integrations)?,
    };
    if selected.is_empty() {
        return Ok(());
    }
    let config = local_config::load(vars)?;
    if config.bind_addr.port() == 0 {
        return Err(
            "bind_addr port 0 is not supported by pontia setup; configure a concrete service port"
                .into(),
        );
    }
    let user_home = validated_user_home(vars)?;
    let vars = integration_vars(vars, &user_home, platform)?;
    platform.preflight()?;
    let prepared = selected
        .iter()
        .map(|&index| integrations[index].prepare(&vars, &user_home, platform.command_runner()))
        .collect::<Result<Vec<_>, _>>()?;
    for setup in &prepared {
        for line in setup.summary() {
            writeln!(output, "  {line}").map_err(|error| error.to_string())?;
        }
        setup.preflight(platform.command_runner())?;
    }
    for setup in &prepared {
        setup.install(platform.command_runner(), platform.definition_store())?;
        writeln!(output, "✓ {}", setup.completion()).map_err(|error| error.to_string())?;
        if let Some(next_steps) = setup.next_steps() {
            writeln!(output, "{next_steps}").map_err(|error| error.to_string())?;
        }
    }
    let token = match config.external_api_token.as_ref() {
        Some(token) => token.clone(),
        None => local_config::generate_token(|bytes| platform.fill_random(bytes))?,
    };
    let changed = local_config::persist_token(&config, &token)?;
    let config = local_config::load(&vars)?;
    let paths = prepared
        .iter()
        .flat_map(|setup| setup.service_environment_paths())
        .collect::<Vec<_>>();
    platform.start_service(&config, changed, &paths)?;
    writeln!(output, "✓ Pontia service configuration verified")
        .map_err(|error| error.to_string())?;
    for client in selected.iter().map(|&index| &integrations[index]) {
        platform
            .enable_client(&config, client.client_type(), client.client_key())
            .map_err(|error| format!("Failed to enable {}: {error}", client.client_type()))?;
        writeln!(output, "✓ Enabled {}", client.client_type())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
