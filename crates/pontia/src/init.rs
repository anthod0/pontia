use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
};

use dialoguer::{MultiSelect, console::Term};
use pontia_config::AppConfig;

use crate::{lifecycle::local_health_addr, local_config};
use pontia_application::client_contract::ClientIntegration;
use pontia_runtime::local_service::{CommandRunner, DefinitionStore};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitOutcome {
    pub local_dashboard_url: String,
}

pub trait InitPlatform {
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
    fn enable_client(&self, config: &AppConfig, client: &str, key: &str) -> Result<(), String>;
    fn dashboard_available(&self, addr: SocketAddr) -> Result<bool, String>;
}

pub fn run<R: BufRead, W: Write, P: InitPlatform>(
    input: &mut R,
    output: &mut W,
    vars: &HashMap<String, String>,
    platform: &P,
) -> Result<Option<InitOutcome>, String> {
    run_with_selector(input, output, vars, platform, line_agent_selection)
}

pub fn run_interactive<P: InitPlatform>(
    vars: &HashMap<String, String>,
    platform: &P,
) -> Result<Option<InitOutcome>, String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_with_selector(
        &mut stdin.lock(),
        &mut stdout.lock(),
        vars,
        platform,
        interactive_agent_selection,
    )
}

fn run_with_selector<R, W, P, S>(
    input: &mut R,
    output: &mut W,
    vars: &HashMap<String, String>,
    platform: &P,
    select_agents: S,
) -> Result<Option<InitOutcome>, String>
where
    R: BufRead,
    W: Write,
    P: InitPlatform,
    S: FnOnce(&mut R, &mut W, &[Arc<dyn ClientIntegration>]) -> Result<Vec<usize>, String>,
{
    let existing = local_config::load(vars)?;
    if existing.bind_addr.port() == 0 {
        return Err(
            "bind_addr port 0 is not supported by pontia init; configure a concrete service port"
                .to_string(),
        );
    }
    let user_home = validated_user_home(vars)?;
    let config_path = existing.pontia_home.join("config.toml");
    let initial_roots = &existing.workspace_browser.roots;

    writeln!(output, "Pontia initialization\n").map_err(io_error)?;
    let integrations = platform.integrations();
    let selected = select_agents(input, output, &integrations)?;
    let prepared = integrations
        .iter()
        .enumerate()
        .map(|(index, client)| {
            selected
                .contains(&index)
                .then(|| client.prepare(vars, &user_home, platform.command_runner()))
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()?;

    let token = match existing.external_api_token.as_deref() {
        Some(token) => token.to_string(),
        None => local_config::generate_token(|bytes| platform.fill_random(bytes))?,
    };

    let mut ignored_overrides = vars
        .iter()
        .filter(|(key, value)| {
            key.starts_with("PONTIA_") && key.as_str() != "PONTIA_HOME" && !value.trim().is_empty()
        })
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    ignored_overrides.sort_unstable();
    if !ignored_overrides.is_empty() {
        writeln!(
            output,
            "Warning: command-scoped overrides are not persisted to the Pontia service and will not be used by initialization: {}",
            ignored_overrides.join(", ")
        )
        .map_err(io_error)?;
    }

    writeln!(output, "\nInitialization summary:").map_err(io_error)?;
    for (client, setup) in integrations.iter().zip(&prepared) {
        let summary = setup
            .as_ref()
            .map_or_else(|| client.skipped_summary(), |setup| setup.summary());
        for line in summary {
            writeln!(output, "  {line}").map_err(io_error)?;
        }
    }
    writeln!(output, "  Workspace Browser roots: {}", initial_roots.len()).map_err(io_error)?;
    writeln!(
        output,
        "  External API token: {}",
        if existing.external_api_token.is_some() {
            "keep existing"
        } else {
            "generate"
        }
    )
    .map_err(io_error)?;
    write!(output, "Continue? [Y/n]: ").map_err(io_error)?;
    match read_answer(input, output)?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "y" | "yes" => {}
        "n" | "no" => {
            writeln!(output, "Initialization cancelled.").map_err(io_error)?;
            return Ok(None);
        }
        answer => return Err(format!("expected yes or no, got {answer:?}")),
    }

    platform.preflight()?;
    for setup in prepared.iter().flatten() {
        setup.preflight(platform.command_runner())?;
    }
    let mut installed = Vec::new();
    let mut install_errors = Vec::new();
    for (client, setup) in integrations.iter().zip(&prepared) {
        let Some(setup) = setup else { continue };
        match setup.install(platform.command_runner(), platform.definition_store()) {
            Ok(()) => {
                installed.push((client, setup));
                writeln!(output, "✓ {}", setup.completion()).map_err(io_error)?;
            }
            Err(error) => install_errors.push(error),
        }
    }
    if installed.is_empty() && !install_errors.is_empty() {
        return Err(install_errors.join("; "));
    }

    let config_changed = local_config::persist_token(&existing, &token)?;
    writeln!(output, "✓ Wrote {}", config_path.display()).map_err(io_error)?;

    let config = local_config::load(vars)?;
    let environment_paths = installed
        .iter()
        .flat_map(|(_, setup)| setup.service_environment_paths())
        .collect::<Vec<_>>();
    platform.start_service(&config, config_changed, &environment_paths)?;
    writeln!(output, "✓ Started Pontia service").map_err(io_error)?;
    // Business settings belong to the running service, not the initialization CLI.
    for (client, _) in installed {
        match platform.enable_client(&config, client.client_type(), client.client_key()) {
            Ok(()) => writeln!(output, "✓ Enabled {}", client.client_type()).map_err(io_error)?,
            Err(error) => install_errors.push(format!(
                "Failed to enable {}: {error}",
                client.client_type()
            )),
        }
    }
    if !install_errors.is_empty() {
        return Err(install_errors.join("; "));
    }

    let dashboard_addr = local_health_addr(config.bind_addr);
    if !platform.dashboard_available(dashboard_addr)? {
        return Err(format!(
            "Dashboard is not available at http://{dashboard_addr}/dashboard"
        ));
    }
    Ok(Some(InitOutcome {
        local_dashboard_url: local_config::dashboard_url(&config)?,
    }))
}

fn line_agent_selection<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    clients: &[Arc<dyn ClientIntegration>],
) -> Result<Vec<usize>, String> {
    writeln!(output, "Select Agent Clients:").map_err(io_error)?;
    for client in clients {
        writeln!(
            output,
            "  [{}] {}",
            if client.selected_by_default() {
                "x"
            } else {
                " "
            },
            client.client_type()
        )
        .map_err(io_error)?;
    }
    let names = clients
        .iter()
        .map(|client| client.client_type())
        .collect::<Vec<_>>();
    let choices = names
        .iter()
        .map(|name| format!("'{name}'"))
        .chain([format!("'{}'", names.join(",")), "'none'".into()])
        .collect::<Vec<_>>()
        .join(", ");
    write!(
        output,
        "Press Enter to keep the defaults, or type a selection ({choices}): "
    )
    .map_err(io_error)?;
    let answer = read_answer(input, output)?;
    let answer = answer.trim();
    if answer.is_empty() {
        return Ok(clients
            .iter()
            .enumerate()
            .filter_map(|(index, client)| client.selected_by_default().then_some(index))
            .collect());
    }
    if answer == "none" {
        return Ok(Vec::new());
    }
    let selected = answer
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();
    if selected.is_empty() || selected.iter().any(|value| !names.contains(value)) {
        return Err(format!("unsupported Agent Client selection: {answer}"));
    }
    Ok(names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| selected.contains(name).then_some(index))
        .collect())
}

fn interactive_agent_selection<R: BufRead, W: Write>(
    _input: &mut R,
    output: &mut W,
    clients: &[Arc<dyn ClientIntegration>],
) -> Result<Vec<usize>, String> {
    output.flush().map_err(io_error)?;
    MultiSelect::new()
        .with_prompt("Select Agent Clients (Space to toggle, Enter to confirm)")
        .items(
            clients
                .iter()
                .map(|client| client.client_type())
                .collect::<Vec<_>>(),
        )
        .defaults(
            &clients
                .iter()
                .map(|client| client.selected_by_default())
                .collect::<Vec<_>>(),
        )
        .interact_on(&Term::stdout())
        .map_err(|error| format!("Agent Client selection failed: {error}"))
}

fn validated_user_home(vars: &HashMap<String, String>) -> Result<PathBuf, String> {
    let path = PathBuf::from(vars.get("HOME").ok_or_else(|| {
        "HOME must be set to install the per-user service and select the default Workspace Browser root"
            .to_string()
    })?);
    validate_root_path(&path)?;
    Ok(path)
}

fn read_answer<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> Result<String, String> {
    output.flush().map_err(io_error)?;
    let mut answer = String::new();
    if input.read_line(&mut answer).map_err(io_error)? == 0 {
        return Err("initialization input ended before confirmation".to_string());
    }
    Ok(answer)
}

fn validate_root_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(format!(
            "Workspace Browser root must be an existing absolute directory: {}",
            path.display()
        ));
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}
