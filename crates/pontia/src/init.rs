use std::{
    collections::{HashMap, HashSet},
    io::{self, BufRead, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
};

use dialoguer::{MultiSelect, console::Term};

use crate::{lifecycle::local_health_addr, local_config, terminal};
use pontia_agent_control::client_contract::ClientIntegration;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitOutcome {
    pub local_dashboard_url: String,
    pub next_steps: Vec<String>,
}

pub trait InitPlatform: crate::setup::SetupPlatform {
    fn dashboard_available(&self, addr: SocketAddr) -> Result<bool, String>;
    fn enabled_clients(
        &self,
        config: &pontia_config::AppConfig,
    ) -> Result<HashSet<(String, String)>, String>;
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
    S: FnOnce(&mut R, &mut W, &[Arc<dyn ClientIntegration>], &[bool]) -> Result<Vec<usize>, String>,
{
    let mut existing = local_config::load(vars)?;
    if existing.bind_addr.port() == 0 {
        return Err(
            "bind_addr port 0 is not supported by pontia init; configure a concrete service port"
                .to_string(),
        );
    }
    let user_home = validated_user_home(vars)?;
    let config_path = existing.pontia_home.join("config.toml");

    writeln!(
        output,
        "{}\n{}\n",
        terminal::heading("Pontia setup"),
        terminal::dim("Connect your coding agents and start the Pontia background service.")
    )
    .map_err(io_error)?;
    let integrations = platform.integrations();
    let enabled =
        if existing.external_api_token.is_some() || existing.pontia_home.join("data").is_dir() {
            writeln!(
                output,
                "Checking the Pontia service to read saved client settings..."
            )
            .map_err(io_error)?;
            platform.preflight()?;
            let config_changed = if existing.external_api_token.is_none() {
                let token = local_config::generate_token(|bytes| platform.fill_random(bytes))?;
                let changed = local_config::persist_token(&existing, &token)?;
                existing = local_config::load(vars)?;
                changed
            } else {
                false
            };
            platform.start_service(&existing, config_changed, &[])?;
            Some(platform.enabled_clients(&existing)?)
        } else {
            None
        };
    let defaults = integrations
        .iter()
        .map(|client| {
            enabled.as_ref().map_or_else(
                || client.selected_by_default(),
                |enabled| {
                    enabled.contains(&(client.client_type().into(), client.client_key().into()))
                },
            )
        })
        .collect::<Vec<_>>();
    let selected = select_agents(input, output, &integrations, &defaults)?;
    let vars = crate::setup::integration_vars(vars, &user_home, platform)?;
    let prepared = integrations
        .iter()
        .enumerate()
        .map(|(index, client)| {
            selected
                .contains(&index)
                .then(|| client.prepare(&vars, &user_home, platform.command_runner()))
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
            key.starts_with("PONTIA_")
                && key.as_str() != "PONTIA_HOME"
                && !value.trim().is_empty()
                && !prepared.iter().flatten().any(|setup| {
                    setup
                        .service_environment_paths()
                        .iter()
                        .any(|(name, _)| name == *key)
                })
        })
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    ignored_overrides.sort_unstable();
    if !ignored_overrides.is_empty() {
        writeln!(
            output,
            "{}",
            terminal::warning(format!(
                "These environment overrides apply only to this command and are not saved for the Pontia service: {}",
                ignored_overrides.join(", ")
            ))
        )
        .map_err(io_error)?;
    }

    writeln!(output, "\n{}", terminal::heading("Pontia will:")).map_err(io_error)?;
    for (client, setup) in integrations.iter().zip(&prepared) {
        let lines = match setup {
            Some(setup) => terminal::summary_lines(&setup.summary(), false),
            None => terminal::summary_lines(&client.skipped_summary(), true),
        };
        for line in lines {
            writeln!(output, "{line}").map_err(io_error)?;
        }
    }
    let settings = [
        format!(
            "Workspace Browser roots: {}",
            existing.workspace_browser.roots.len()
        ),
        format!(
            "External API token: {}",
            if existing.external_api_token.is_some() {
                "keep existing"
            } else {
                "generate"
            }
        ),
    ];
    for line in settings {
        writeln!(output, "  • {line}").map_err(io_error)?;
    }
    write!(output, "\n{}", terminal::question("Continue?", "[Y/n]")).map_err(io_error)?;
    match read_answer(input, output)?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "y" | "yes" => {}
        "n" | "no" => {
            let message = if enabled.is_some() {
                "Client setup cancelled. The Pontia service remains available."
            } else {
                "Setup cancelled. Nothing was changed."
            };
            writeln!(output, "{message}").map_err(io_error)?;
            return Ok(None);
        }
        answer => return Err(format!("expected yes or no, got {answer:?}")),
    }

    platform.preflight()?;
    for setup in prepared.iter().flatten() {
        setup.preflight(platform.command_runner())?;
    }
    writeln!(output).map_err(io_error)?;
    let mut installed = Vec::new();
    let mut install_errors = Vec::new();
    for (client, setup) in integrations.iter().zip(&prepared) {
        let Some(setup) = setup else { continue };
        match setup.install(platform.command_runner(), platform.definition_store()) {
            Ok(()) => {
                installed.push((client, setup));
                writeln!(output, "{}", terminal::success(setup.completion())).map_err(io_error)?;
            }
            Err(error) => install_errors.push(error),
        }
    }
    if installed.is_empty() && !install_errors.is_empty() {
        return Err(install_errors.join("; "));
    }

    let config_changed = local_config::persist_token(&existing, &token)?;
    writeln!(
        output,
        "{}",
        terminal::success(if config_changed {
            format!("Saved configuration to {}", config_path.display())
        } else {
            format!("Configuration unchanged: {}", config_path.display())
        })
    )
    .map_err(io_error)?;

    let config = local_config::load(&vars)?;
    let environment_paths = installed
        .iter()
        .flat_map(|(_, setup)| setup.service_environment_paths())
        .collect::<Vec<_>>();
    platform.start_service(&config, config_changed, &environment_paths)?;
    writeln!(output, "{}", terminal::success("Pontia service is ready")).map_err(io_error)?;
    // Business settings belong to the running service, not the initialization CLI.
    for (client, _) in &installed {
        if enabled.as_ref().is_some_and(|enabled| {
            enabled.contains(&(client.client_type().into(), client.client_key().into()))
        }) {
            continue;
        }
        match platform.enable_client(&config, client.client_type(), client.client_key()) {
            Ok(()) => writeln!(
                output,
                "{}",
                terminal::success(format!("Enabled {}", client.client_type()))
            )
            .map_err(io_error)?,
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
        next_steps: installed
            .iter()
            .filter_map(|(_, setup)| setup.next_steps())
            .collect(),
    }))
}

pub(crate) fn line_agent_selection<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    clients: &[Arc<dyn ClientIntegration>],
    defaults: &[bool],
) -> Result<Vec<usize>, String> {
    writeln!(
        output,
        "{}",
        terminal::heading("Agent Clients to configure or check this time:")
    )
    .map_err(io_error)?;
    for (client, selected) in clients.iter().zip(defaults) {
        writeln!(
            output,
            "  [{}] {}",
            if *selected { "x" } else { " " },
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
        "{}",
        terminal::question(
            "Press Enter to keep the defaults, or type a selection",
            &format!("({choices})")
        )
    )
    .map_err(io_error)?;
    let answer = read_answer(input, output)?;
    let answer = answer.trim();
    if answer.is_empty() {
        return Ok(defaults
            .iter()
            .enumerate()
            .filter_map(|(index, selected)| selected.then_some(index))
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

pub(crate) fn interactive_agent_selection<R: BufRead, W: Write>(
    _input: &mut R,
    output: &mut W,
    clients: &[Arc<dyn ClientIntegration>],
    defaults: &[bool],
) -> Result<Vec<usize>, String> {
    output.flush().map_err(io_error)?;
    MultiSelect::with_theme(&terminal::theme())
        .with_prompt("Clients to configure or check this time (Space to toggle, Enter to confirm)")
        .items(
            clients
                .iter()
                .map(|client| client.client_type())
                .collect::<Vec<_>>(),
        )
        .defaults(defaults)
        .interact_on(&Term::stdout())
        .map_err(|error| format!("Agent Client selection failed: {error}"))
}

pub(crate) fn validated_user_home(vars: &HashMap<String, String>) -> Result<PathBuf, String> {
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
