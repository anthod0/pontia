mod bot;
mod diagnostics;
mod login;
mod remote;
mod shutdown;
mod update;
mod workflow;
mod workspace;

use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Component, Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode, Stdio},
    time::Duration,
};

use clap::{
    CommandFactory, FromArgMatches, Parser, Subcommand,
    builder::{StyledStr, TypedValueParser},
};
use dialoguer::Confirm;
use pontia::{
    init::{self, InitPlatform},
    lifecycle::{Lifecycle, RemoteServiceState, RunState, ServiceManager, UpOptions},
    manager::ProcessCommandRunner,
    runtime_io::{FileDefinitionStore, HttpHealthProbe},
    setup::{self, SetupPlatform},
    terminal,
};
use pontia_agent_control::client_contract::ClientIntegration;
use pontia_config::AppConfig;
use pontia_runtime::local_service::service_executable_search_path;
use pontia_runtime::local_service::{CommandRunner, DefinitionStore};
use std::sync::Arc;

#[cfg(target_os = "macos")]
use pontia::manager::LaunchdManager;
#[cfg(target_os = "linux")]
use pontia::{
    lifecycle::{HealthProbe, local_health_addr},
    manager::SystemdManager,
};

const ROOT_HELP: &str = "\
Configuration and state live in $PONTIA_HOME (default: ~/.pontia).

For documentation, see https://pontia.dev/llms.txt";

const AGENT_TOOLS: [&str; 2] = ["workflow", "bot"];

const INIT_HELP: &str = "\
Prompts on the terminal. Rerunning checks the service and defaults to enabled clients.
Keeps the existing API token and Workspace roots. Unselected clients are left unchanged.
Use 'pontia setup [CLIENT]' to install or repair one integration later.

For setup without prompts, see https://pontia.dev/docs/initialization.md#non-interactive-initialization";

const LOGIN_HELP: &str = "\
Prints a URL and a one-time code, then waits until the user approves them in a browser.
Saves the credential to $PONTIA_HOME/auth.json.";

const UP_HELP: &str = "\
Creates the API token if missing and restarts a running service.
Refreshes the service PATH from this terminal, keeping absolute directories in order.
Run after editing $PONTIA_HOME/config.toml. The printed Dashboard URL contains the token.

For configuration options, see https://pontia.dev/docs/configuration.md";

const UPDATE_HELP: &str = "\
Restarts a running service. Agents continue working during the restart.
To install a specific release, see https://pontia.dev/docs/updating.md";

const DOWN_HELP: &str = "Configuration and state are kept; run 'pontia up' to start again.";

const COMPLETIONS_HELP: &str = "\
bash: add 'source <(pontia completions bash)' to ~/.bashrc
zsh:  add 'source <(pontia completions zsh)' to ~/.zshrc
fish: pontia completions fish > ~/.config/fish/completions/pontia.fish";

const STATUS_HELP: &str = "\
Exits 0 only when the service is running and HTTP is healthy.

Run 'pontia doctor' for detailed checks and repair hints.";

#[derive(Debug, Parser)]
#[command(
    name = "pontia",
    version = pontia_version::version(),
    about = "Control Pontia from the command line",
    after_help = ROOT_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(hide = true)]
    WorkspaceCheck { path: PathBuf },
    /// Configure Pontia interactively and start its per-user service
    #[command(after_help = INIT_HELP)]
    Init,
    /// Install or repair local Agent Client integrations
    #[command(
        after_help = "Select clients when omitted. Configures and enables selected clients.\nRun 'pontia init' to initialize or check the whole installation."
    )]
    Setup {
        /// Agent Client to configure
        client: Option<String>,
    },
    #[command(hide = true)]
    InstallSetup { client: String },
    #[command(hide = true)]
    SetupWorker {
        client: String,
        operation_id: uuid::Uuid,
    },
    /// Manage Agent Client settings through the Pontia service
    Client(pontia::client::ClientCommand),
    /// Sign in to the Pontia cloud from a headless terminal
    #[command(after_help = LOGIN_HELP)]
    Login,
    /// Configure remote device access
    Remote(remote::RemoteCommand),
    /// Install and start the per-user Pontia service
    #[command(after_help = UP_HELP)]
    Up,
    /// Update pontia and pontiad to the latest stable release
    #[command(after_help = UPDATE_HELP)]
    Update,
    /// Manage six-hour automatic update checks (disabled by default)
    #[command(subcommand)]
    AutoUpdate(AutoUpdate),
    /// Remove local integrations, shell configuration, and autostart services
    #[command(
        after_help = "Keeps pontia and pontiad, configuration, credentials, history, and cloud registration.\nAgent Client installations and native conversations are kept. Run pontia init to set up again."
    )]
    Uninstall {
        /// Apply the displayed cleanup without prompting
        #[arg(short = 'y', long)]
        yes: bool,
        /// Stop agents started by Pontia before removing integrations
        #[arg(long)]
        force: bool,
    },
    /// Stop and disable the per-user Pontia service
    #[command(after_help = DOWN_HELP)]
    Down {
        /// Stop agents started by Pontia before stopping the service
        #[arg(long)]
        force: bool,
    },
    /// Show service health, the signed-in user, and remote registration
    #[command(after_help = STATUS_HELP)]
    Status,
    /// Check service, configuration, login, and remote access
    Doctor,
    /// Print a shell completion script
    #[command(after_help = COMPLETIONS_HELP)]
    Completions {
        /// Shell to generate the script for
        shell: clap_complete::Shell,
    },
    /// Run and interact with Workflows
    Workflow(workflow::WorkflowCommand),
    /// Use tools for the current Bot, retrieval or dreaming Session
    Bot(bot::BotCommand),
}

#[derive(Debug, Subcommand)]
enum AutoUpdate {
    /// Install and start the per-user update schedule
    Enable,
    /// Stop and remove the per-user update schedule
    Disable,
    /// Show the schedule and available run times
    Status,
}

#[derive(Debug)]
enum LifecycleCommand {
    Up,
    Down { force: bool },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::from_arg_matches(&cli_command().get_matches()).unwrap_or_else(|e| e.exit());
    match execute(cli.command).await {
        Ok(operational) if operational => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("pontia: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Lists Agent tools in their own section of `pontia --help`, after the commands for people.
fn cli_command() -> clap::Command {
    use std::fmt::Write as _;

    let mut command = Cli::command();
    let styles = command.get_styles();
    let (header, literal) = (styles.get_header(), styles.get_literal());
    let width = AGENT_TOOLS.iter().map(|name| name.len()).max().unwrap_or(0);
    let mut template = StyledStr::new();
    let _ = write!(
        template,
        "{{before-help}}{{about-with-newline}}\n{{usage-heading}} {{usage}}\n\n\
         {header}Commands:{header:#}\n{{subcommands}}\n\n\
         {header}Agent tools:{header:#}"
    );
    for name in AGENT_TOOLS {
        let about = command
            .find_subcommand(name)
            .and_then(clap::Command::get_about)
            .map(StyledStr::to_string)
            .unwrap_or_default();
        let _ = write!(template, "\n  {literal}{name:<width$}{literal:#}  {about}");
    }
    let _ = write!(
        template,
        "\n\n{header}Options:{header:#}\n{{options}}{{after-help}}"
    );
    for name in AGENT_TOOLS {
        command = command.mut_subcommand(name, |tool| tool.hide(true));
    }
    let integrations = pontia_clients::integrations();
    let clients: Vec<&'static str> = integrations
        .iter()
        .map(|client| client.setup_name())
        .collect();
    command = command.mut_subcommand("setup", |setup| {
        setup.mut_arg("client", |arg| {
            arg.value_parser(
                clap::builder::PossibleValuesParser::new(clients).map(move |name| {
                    integrations
                        .iter()
                        .find(|client| client.setup_name() == name)
                        .expect("validated setup name")
                        .client_type()
                        .to_string()
                }),
            )
        })
    });
    command.help_template(template)
}

async fn execute(command: Command) -> Result<bool, String> {
    match command {
        Command::WorkspaceCheck { path } => {
            let config = pontia_clients::config_from_env().map_err(|error| error.to_string())?;
            workspace::is_active(&config, &path).await
        }
        Command::Init => run_init().await,
        Command::Setup { client } => {
            let vars = env::vars().collect();
            setup::run_interactive(client.as_deref(), &vars, &RealInitPlatform)?;
            Ok(true)
        }
        Command::InstallSetup { client } => {
            setup::installed(
                &client,
                &mut std::io::stdout(),
                &env::vars().collect(),
                &RealInitPlatform,
            )?;
            Ok(true)
        }
        Command::SetupWorker {
            client,
            operation_id,
        } => {
            let result = (|| {
                let integration = pontia_clients::integrations()
                    .into_iter()
                    .find(|entry| entry.client_type() == client)
                    .ok_or_else(|| format!("Unknown setup integration: {client}"))?;
                let home = env::var_os("PONTIA_HOME")
                    .map(PathBuf::from)
                    .ok_or("PONTIA_HOME is required")?;
                let cli = env::current_exe().map_err(|error| error.to_string())?;
                pontia_runtime::client_setup::run_worker(
                    &home,
                    &cli,
                    &client,
                    integration.client_key(),
                    integration.setup_name(),
                    &operation_id.to_string(),
                    &mut std::io::stdout(),
                )
            })();
            if let Err(error) = &result {
                println!(
                    "{}",
                    serde_json::to_string(&Err::<(), _>(error))
                        .map_err(|error| error.to_string())?
                );
            }
            result.map(|_| true)
        }
        Command::Client(command) => {
            let config = pontia_clients::config_from_env().map_err(|error| error.to_string())?;
            println!("{}", pontia::client::run(command, &config).await?);
            Ok(true)
        }
        Command::Login => {
            let vars: HashMap<String, String> = env::vars().collect();
            login::run(&vars).await?;
            Ok(true)
        }
        Command::Bot(command) => {
            bot::run(command).await?;
            Ok(true)
        }
        Command::Workflow(command) => {
            let config = pontia_clients::config_from_env().map_err(|error| error.to_string())?;
            workflow::run(command, &config).await?;
            Ok(true)
        }
        Command::Remote(command) => {
            let vars: HashMap<String, String> = env::vars().collect();
            match remote::run(command, &vars).await? {
                remote::ServiceRestart::Required => restart_service_for_remote_config()?,
                remote::ServiceRestart::NotRequired => {}
                remote::ServiceRestart::ApplyEdge { edge_id } => {
                    let outcome = apply_remote_edge().map_err(|error| format!(
                        "Edge target {edge_id} is saved, but the service could not apply it: {error}. Fix the service and run `pontia up` to apply the saved target (or restart an unmanaged pontiad manually)."
                    ))?;
                    match outcome {
                        RemoteServiceState::Restarted => println!(
                            "Service restarted. Remote connectivity has not been verified; reconnect or refresh the Dashboard."
                        ),
                        RemoteServiceState::Stopped => println!(
                            "Service is stopped. The saved Edge target will apply on the next `pontia up`."
                        ),
                        RemoteServiceState::Unmanaged => println!(
                            "The service for this configuration is not managed here. Restart pontiad manually to apply the saved Edge target, then refresh the Dashboard."
                        ),
                    }
                }
            }
            Ok(true)
        }
        Command::AutoUpdate(command) => run_auto_update(command),
        Command::Update => run_update().await,
        Command::Uninstall { yes, force } => run_uninstall(yes, force).await,
        Command::Up => run_lifecycle(LifecycleCommand::Up).await,
        Command::Down { force } => run_lifecycle(LifecycleCommand::Down { force }).await,
        Command::Status => run_diagnostics(false).await,
        Command::Doctor => run_diagnostics(true).await,
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut cli_command(), "pontia", &mut std::io::stdout());
            Ok(true)
        }
    }
}

async fn run_uninstall(yes: bool, force: bool) -> Result<bool, String> {
    let runner = ProcessCommandRunner;
    #[cfg(target_os = "linux")]
    let manager = SystemdManager::with_environment_paths(
        &runner,
        pontia_clients::service_path_variables(),
        &[],
    );
    #[cfg(target_os = "macos")]
    let manager = launchd_manager(&runner, &[])?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let prepared = pontia::uninstall::PreparedUninstall::prepare(
            &user_home()?,
            &env::vars().collect(),
            &manager,
            &runner,
            &pontia_clients::integrations(),
        )?;
        let remaining_data_hint = prepared.remaining_data_hint();
        let config = prepared.service_config()?;
        let mut empty = false;
        let list = |agents: Vec<String>| {
            let mut summary = prepared.summary();
            summary.extend(agents);
            empty = summary.is_empty();
            if empty {
                println!("No installed Pontia integrations or services to remove.");
                println!("{remaining_data_hint}");
            } else {
                for line in summary {
                    println!("  • {line}");
                }
                println!(
                    "Binaries, configuration, credentials, history, and cloud registration will be kept."
                );
            }
        };
        use shutdown::{Operation, run};
        let mut guard = run(&config, Operation::Uninstall, force, yes, &manager, list).await?;
        if empty {
            return Ok(true);
        }
        if !yes {
            if !Confirm::with_theme(&terminal::theme())
                .with_prompt("Remove these Pontia integrations and services?")
                .default(false)
                .interact()
                .map_err(|error| error.to_string())?
            {
                return Ok(true);
            }
            guard = run(&config, Operation::Uninstall, force, true, &manager, |_| {}).await?;
        }
        prepared.uninstall(&manager, &runner)?;
        drop(guard);
        println!(
            "{}",
            terminal::success(
                "Removed Pontia integrations and autostart services. Open a new terminal to refresh PATH."
            )
        );
        println!("{remaining_data_hint}");
        Ok(true)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Err("pontia uninstall supports only Linux and macOS".into())
}

fn run_auto_update(command: AutoUpdate) -> Result<bool, String> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let runner = ProcessCommandRunner;
        #[cfg(target_os = "linux")]
        let (manager, directory) = (
            SystemdManager::new(&runner),
            match env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
                Some(path) if path.is_absolute() => path,
                _ => user_home()?.join(".config"),
            }
            .join("systemd/user"),
        );
        #[cfg(target_os = "macos")]
        let (manager, directory) = (
            LaunchdManager::new(&runner, current_uid(&runner)?),
            user_home()?.join("Library/LaunchAgents"),
        );
        match command {
            AutoUpdate::Enable => {
                let cli = env::current_exe().map_err(|error| error.to_string())?;
                manager.enable_auto_update(&directory, &cli)?;
                println!("Automatic updates enabled.");
            }
            AutoUpdate::Disable => {
                manager.disable_auto_update(&directory)?;
                println!("Automatic updates disabled.");
            }
            AutoUpdate::Status => println!("{}", manager.auto_update_status()?),
        }
        Ok(true)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Err(format!(
        "Automatic updates require systemd on Linux or launchd on macOS ({command:?})."
    ))
}

async fn run_update() -> Result<bool, String> {
    #[cfg(target_os = "linux")]
    {
        let runner = ProcessCommandRunner;
        let manager = SystemdManager::with_environment_paths(
            &runner,
            pontia_clients::service_path_variables(),
            &[],
        );
        // Resolve the installed service's configuration, not the invoking shell's PONTIA_HOME.
        let config = if Path::new("/run/systemd/system").is_dir() {
            update_service_config(&manager, &user_home()?, false)?
        } else {
            None
        };
        let Some(update) = update::prepare().await? else {
            maintain_updated_clients(config.as_ref(), &manager, pontia_version::version()).await?;
            return Ok(true);
        };
        if config.is_some() && manager.running_executable()? != update.daemon_path() {
            return Err("the running service belongs to another Pontia installation; run its sibling pontia update instead".into());
        }
        if config.is_none() {
            update.ensure_no_unmanaged_daemon()?;
        }
        let version = update.install(|| {
            if let Some(config) = &config {
                eprintln!("Restarting Pontia and waiting for it to become healthy...");
                Lifecycle::new(&manager, &FileDefinitionStore, &HttpHealthProbe).restart(config)?;
            }
            Ok(())
        })?;
        maintain_updated_clients(config.as_ref(), &manager, &version).await?;
        Ok(true)
    }
    #[cfg(target_os = "macos")]
    {
        let runner = ProcessCommandRunner;
        let manager = launchd_manager(&runner, &[])?;
        // A loaded KeepAlive job can restart while its process is temporarily absent,
        // so keep treating stopped or failed loaded jobs as managed during replacement.
        let config = update_service_config(&manager, &user_home()?, true)?;
        let Some(update) = update::prepare().await? else {
            maintain_updated_clients(config.as_ref(), &manager, pontia_version::version()).await?;
            return Ok(true);
        };
        if config.is_some() && manager.running_executable()? != update.daemon_path() {
            return Err("the running service belongs to another Pontia installation; run its sibling pontia update instead".into());
        }
        if config.is_none() {
            update.ensure_no_unmanaged_daemon()?;
        }
        let version = update.install(|| {
            if let Some(config) = &config {
                eprintln!("Restarting Pontia and waiting for it to become healthy...");
                Lifecycle::new(&manager, &FileDefinitionStore, &HttpHealthProbe).restart(config)?;
            }
            Ok(())
        })?;
        maintain_updated_clients(config.as_ref(), &manager, &version).await?;
        Ok(true)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err("pontia update supports only Linux and macOS".into())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn maintain_updated_clients(
    config: Option<&AppConfig>,
    manager: &impl ServiceManager,
    version: &str,
) -> Result<(), String> {
    let Some(config) = config else {
        eprintln!("Client integration maintenance skipped: no managed service was restored.");
        return Ok(());
    };
    let entries = pontia::client::ClientApi::new(config)?.list().await?;
    let home = user_home()?;
    let definition = FileDefinitionStore
        .read(&manager.definition_path(&home))?
        .ok_or("The restored service definition is missing")?;
    let mut vars: HashMap<String, String> = env::vars().collect();
    for name in pontia_clients::service_path_variables() {
        vars.remove(name);
    }
    for (name, path) in manager.persisted_client_paths(&definition)? {
        vars.insert(name, path.to_string_lossy().into_owned());
    }
    vars.insert(
        "PONTIA_HOME".into(),
        config.pontia_home.display().to_string(),
    );
    pontia::client_maintenance::maintain_enabled(
        &entries,
        &pontia_clients::integrations_for_version(version),
        &vars,
        &home,
        &ProcessCommandRunner,
        &FileDefinitionStore,
    )
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn update_service_config<M: ServiceManager>(
    manager: &M,
    home: &Path,
    restart_loaded: bool,
) -> Result<Option<AppConfig>, String> {
    use pontia::lifecycle::DefinitionStore;
    let Some(definition) = FileDefinitionStore.read(&manager.definition_path(home))? else {
        return Ok(None);
    };
    let status = manager.status()?;
    match status.run_state {
        RunState::Starting => {
            return Err("Pontia is starting; retry the update once it has settled".into());
        }
        RunState::Running => {}
        RunState::Stopped | RunState::Failed if restart_loaded && status.loaded => {}
        RunState::Stopped | RunState::Failed => return Ok(None),
    }
    let pontia_home = manager.persisted_home(&definition)?;
    pontia_clients::config_from_vars(&HashMap::from([(
        "PONTIA_HOME".to_string(),
        pontia_home.display().to_string(),
    )]))
    .map(Some)
    .map_err(|error| error.to_string())
}

async fn run_lifecycle(command: LifecycleCommand) -> Result<bool, String> {
    service_manager_preflight()?;
    let runner = ProcessCommandRunner;
    let search_path = match command {
        LifecycleCommand::Up => service_executable_search_path(env::var_os("PATH").as_deref())?,
        LifecycleCommand::Down { .. } => Vec::new(),
    };

    #[cfg(target_os = "linux")]
    {
        run_with_manager(
            command,
            &SystemdManager::with_environment_paths(
                &runner,
                pontia_clients::service_path_variables(),
                &[],
            )
            .with_executable_search_path(search_path),
        )
        .await
    }

    #[cfg(target_os = "macos")]
    {
        let manager = launchd_manager(&runner, &[])?.with_executable_search_path(search_path);
        run_with_manager(command, &manager).await
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (command, runner);
        Err(
            "automatic lifecycle management is supported only on Linux with systemd and macOS"
                .to_string(),
        )
    }
}

fn apply_remote_edge() -> Result<RemoteServiceState, String> {
    let config = pontia_clients::config_from_env().map_err(|error| error.to_string())?;
    let definitions = FileDefinitionStore;
    let health = HttpHealthProbe;
    let runner = ProcessCommandRunner;
    #[cfg(target_os = "linux")]
    {
        if !Path::new("/run/systemd/system").is_dir() {
            return Ok(if health.is_healthy(local_health_addr(config.bind_addr))? {
                RemoteServiceState::Unmanaged
            } else {
                RemoteServiceState::Stopped
            });
        }
        let manager = SystemdManager::new(&runner);
        Lifecycle::new(&manager, &definitions, &health).apply_remote_target(&config, &user_home()?)
    }
    #[cfg(target_os = "macos")]
    {
        let manager = launchd_manager(&runner, &[])?;
        Lifecycle::new(&manager, &definitions, &health).apply_remote_target(&config, &user_home()?)
    }
}

fn restart_service_for_remote_config() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if !Path::new("/run/systemd/system").is_dir() {
            return Ok(());
        }
        let runner = ProcessCommandRunner;
        restart_with_manager_if_running(&SystemdManager::with_environment_paths(
            &runner,
            pontia_clients::service_path_variables(),
            &[],
        ))
    }

    #[cfg(target_os = "macos")]
    {
        let runner = ProcessCommandRunner;
        restart_with_manager_if_running(&launchd_manager(&runner, &[])?)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Ok(())
}

fn restart_with_manager_if_running<M: ServiceManager>(manager: &M) -> Result<(), String> {
    if manager.status()?.run_state != RunState::Running {
        return Ok(());
    }
    let config = pontia_clients::config_from_env().map_err(|error| error.to_string())?;
    let definitions = FileDefinitionStore;
    let health = HttpHealthProbe;
    Lifecycle::new(manager, &definitions, &health).up(
        &config,
        &sibling_pontiad()?,
        &user_home()?,
        UpOptions {
            restart_running: true,
        },
    )
}

fn service_manager_preflight() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if !Path::new("/run/systemd/system").is_dir() {
        return Err(
            "automatic lifecycle management requires a running systemd user service manager; supervise pontiad with OpenRC, runit, or s6 on this Linux system"
                .to_string(),
        );
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Err(
        "automatic lifecycle management is supported only on Linux with systemd and macOS"
            .to_string(),
    );

    Ok(())
}

async fn run_with_manager<M: ServiceManager>(
    command: LifecycleCommand,
    manager: &M,
) -> Result<bool, String> {
    let definitions = FileDefinitionStore;
    let health = HttpHealthProbe;
    let lifecycle = Lifecycle::new(manager, &definitions, &health);

    match command {
        LifecycleCommand::Up => {
            let vars = std::env::vars().collect();
            let config = pontia::local_config::prepare(&vars, |bytes| {
                getrandom::fill(bytes)
                    .map_err(|error| format!("failed to generate a secure token: {error}"))
            })?;
            eprintln!("Starting Pontia service and waiting for it to become healthy...");
            start_with_lifecycle(
                &lifecycle,
                &config,
                UpOptions {
                    restart_running: true,
                },
            )?;
            let dashboard_addr = pontia::lifecycle::local_health_addr(config.bind_addr);
            if !dashboard_available(dashboard_addr)? {
                return Err(format!(
                    "Dashboard is not available at http://{dashboard_addr}/dashboard"
                ));
            }
            let dashboard_url = pontia::local_config::dashboard_url(&config)?;
            println!("Pontia is up and healthy.");
            println!("Local Dashboard: {dashboard_url}");
            Ok(true)
        }
        LifecycleCommand::Down { force } => {
            let definition = definitions.read(&manager.definition_path(&user_home()?))?;
            let config = match definition {
                Some(definition) => pontia_clients::config_from_vars(&HashMap::from([(
                    "PONTIA_HOME".into(),
                    manager.persisted_home(&definition)?.display().to_string(),
                )])),
                None => pontia_clients::config_from_env(),
            }
            .map_err(|e| e.to_string())?;
            let _guard = shutdown::run(
                &config,
                shutdown::Operation::Down,
                force,
                true,
                manager,
                |_| {},
            )
            .await?;
            lifecycle.down()?;
            println!("Pontia is down.");
            Ok(true)
        }
    }
}

fn start_with_lifecycle<M: ServiceManager>(
    lifecycle: &Lifecycle<'_, M, FileDefinitionStore, HttpHealthProbe>,
    config: &AppConfig,
    options: UpOptions,
) -> Result<(), String> {
    lifecycle.up(config, &sibling_pontiad()?, &user_home()?, options)
}

struct RealInitPlatform;

impl SetupPlatform for RealInitPlatform {
    fn integrations(&self) -> Vec<Arc<dyn ClientIntegration>> {
        pontia_clients::integrations()
    }
    fn command_runner(&self) -> &dyn CommandRunner {
        &ProcessCommandRunner
    }
    fn definition_store(&self) -> &dyn DefinitionStore {
        &FileDefinitionStore
    }
    fn preflight(&self) -> Result<(), String> {
        service_manager_preflight()?;
        sibling_pontiad()?;
        Ok(())
    }

    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), String> {
        getrandom::fill(bytes)
            .map_err(|error| format!("failed to generate a secure token: {error}"))
    }

    fn start_service(
        &self,
        config: &AppConfig,
        config_changed: bool,
        environment_paths: &[(String, PathBuf)],
    ) -> Result<(), String> {
        service_manager_preflight()?;
        start_init_service(config, config_changed, environment_paths)
    }

    fn persisted_environment_paths(&self, home: &Path) -> Result<Vec<(String, PathBuf)>, String> {
        let runner = ProcessCommandRunner;
        #[cfg(target_os = "linux")]
        let manager = SystemdManager::with_environment_paths(
            &runner,
            pontia_clients::service_path_variables(),
            &[],
        );
        #[cfg(target_os = "macos")]
        let manager = launchd_manager(&runner, &[])?;
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            FileDefinitionStore
                .read(&manager.definition_path(home))?
                .map(|definition| manager.persisted_client_paths(&definition))
                .transpose()
                .map(Option::unwrap_or_default)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        Err("automatic lifecycle management is unavailable".into())
    }

    fn enable_client(&self, config: &AppConfig, client: &str, key: &str) -> Result<(), String> {
        let config = config.clone();
        let client = client.to_string();
        let key = key.to_string();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .map_err(|error| error.to_string())?
                .block_on(async {
                    pontia::client::ClientApi::new(&config)?
                        .patch(&client, &key, serde_json::json!({"enabled": true}))
                        .await
                        .map(|_| ())
                })
        })
        .join()
        .map_err(|_| "Client enable request panicked".to_string())?
    }
}

impl InitPlatform for RealInitPlatform {
    fn enabled_clients(
        &self,
        config: &AppConfig,
    ) -> Result<std::collections::HashSet<(String, String)>, String> {
        let config = config.clone();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .map_err(|error| error.to_string())?
                .block_on(async {
                    Ok(pontia::client::ClientApi::new(&config)?
                        .list()
                        .await?
                        .into_iter()
                        .filter(|client| client.enabled)
                        .map(|client| (client.client_type, client.client_key))
                        .collect())
                })
        })
        .join()
        .map_err(|_| "Client settings request panicked".to_string())?
    }

    fn dashboard_available(&self, addr: SocketAddr) -> Result<bool, String> {
        dashboard_available(addr)
    }
}

fn start_init_with_manager<M: ServiceManager>(
    manager: &M,
    config: &AppConfig,
    config_changed: bool,
) -> Result<(), String> {
    let definitions = FileDefinitionStore;
    let health = HttpHealthProbe;
    let lifecycle = Lifecycle::new(manager, &definitions, &health);
    start_with_lifecycle(
        &lifecycle,
        config,
        UpOptions {
            restart_running: config_changed,
        },
    )
}

#[cfg(target_os = "linux")]
fn start_init_service(
    config: &AppConfig,
    config_changed: bool,
    environment_paths: &[(String, PathBuf)],
) -> Result<(), String> {
    let runner = ProcessCommandRunner;
    start_init_with_manager(
        &SystemdManager::with_environment_paths(
            &runner,
            pontia_clients::service_path_variables(),
            environment_paths,
        )
        .with_executable_search_path(service_executable_search_path(
            env::var_os("PATH").as_deref(),
        )?),
        config,
        config_changed,
    )
}

#[cfg(target_os = "macos")]
fn start_init_service(
    config: &AppConfig,
    config_changed: bool,
    environment_paths: &[(String, PathBuf)],
) -> Result<(), String> {
    let runner = ProcessCommandRunner;
    start_init_with_manager(
        &launchd_manager(&runner, environment_paths)?.with_executable_search_path(
            service_executable_search_path(env::var_os("PATH").as_deref())?,
        ),
        config,
        config_changed,
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn start_init_service(
    _config: &AppConfig,
    _config_changed: bool,
    _environment_paths: &[(String, PathBuf)],
) -> Result<(), String> {
    Err("automatic lifecycle management is unavailable".to_string())
}

#[cfg(target_os = "linux")]
fn open_browser(url: &str) -> Result<(), String> {
    run_browser_opener("xdg-open", url)
}

#[cfg(target_os = "macos")]
fn open_browser(url: &str) -> Result<(), String> {
    run_browser_opener("open", url)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn open_browser(_url: &str) -> Result<(), String> {
    Err("browser opening is supported only on Linux and macOS".to_string())
}

fn run_browser_opener(program: &str, url: &str) -> Result<(), String> {
    let status = ProcessCommand::new(program)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("failed to launch the browser opener: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("browser opener failed with {status}"))
    }
}

async fn run_init() -> Result<bool, String> {
    let vars: HashMap<String, String> = env::vars().collect();
    let Some(outcome) = init::run_interactive(&vars, &RealInitPlatform)? else {
        return Ok(true);
    };

    println!();
    let enable_remote = Confirm::with_theme(&terminal::theme())
        .with_prompt("Sign in to Pontia Cloud to use this device from anywhere?")
        .default(true)
        .interact()
        .map_err(|error| format!("failed to read the remote access selection: {error}"))?;

    let result = if !enable_remote {
        println!(
            "\n{}",
            terminal::dim(format!(
                "Remote access skipped. To enable it later, run {} and then {}.",
                terminal::command("pontia login"),
                terminal::command("pontia remote enable")
            ))
        );
        open_dashboard("Local Dashboard", &outcome.local_dashboard_url);
        Ok(true)
    } else {
        println!();
        match enable_remote_access(&vars).await {
            Ok(()) => Ok(true),
            Err(error) => {
                println!(
                    "\n{}",
                    terminal::warning(
                        "Remote access setup did not finish. The local setup is complete."
                    )
                );
                println!(
                    "  Retry with {} and then {}.",
                    terminal::command("pontia login"),
                    terminal::command("pontia remote enable")
                );
                open_dashboard("Local Dashboard", &outcome.local_dashboard_url);
                Err(error)
            }
        }
    };
    for next_steps in outcome.next_steps {
        println!("\n{}", terminal::heading("Next step"));
        println!("{}", terminal::indent(&next_steps));
    }
    println!(
        "\n{}",
        terminal::dim(format!(
            "Pontia keeps running in the background. Run {} to stop it.",
            terminal::command("pontia down")
        ))
    );
    result
}

async fn enable_remote_access(vars: &HashMap<String, String>) -> Result<(), String> {
    let planned_id = if !login::has_valid_credential(vars)? {
        let id = remote::planned_device_id(&login::pontia_home(vars)?.join("config.toml"))?;
        login::run_with_verification(vars, Some(&id), |url| {
            let _ = open_browser(url);
        })
        .await?;
        Some(id)
    } else {
        println!("{}", terminal::success("Already signed in to Pontia Cloud"));
        None
    };
    let needs_approval = planned_id.is_some();
    let access = remote::enable(vars, planned_id).await?;
    if matches!(access.restart, remote::ServiceRestart::Required) {
        restart_service_for_remote_config()?;
    }
    let url = remote_dashboard_url(&access.device_handle)?;
    if needs_approval {
        print_dashboard_url("Remote Dashboard", &url);
    } else {
        open_dashboard("Remote Dashboard", &url);
    }
    Ok(())
}

fn remote_dashboard_url(device_handle: &str) -> Result<String, String> {
    let mut url = url::Url::parse("https://app.pontia.dev/")
        .map_err(|error| format!("failed to build the remote Dashboard URL: {error}"))?;
    url.path_segments_mut()
        .map_err(|_| "failed to build the remote Dashboard URL".to_string())?
        .push(device_handle);
    Ok(url.to_string())
}

fn print_dashboard_url(label: &str, url: &str) {
    println!(
        "\n{}",
        terminal::heading(format!("Pontia is ready. Open the {label}:"))
    );
    println!("\n  {}\n", terminal::link(url));
}

fn open_dashboard(label: &str, url: &str) {
    print_dashboard_url(label, url);
    if open_browser(url).is_ok() {
        println!("{}", terminal::success("Opened in your browser"));
    } else {
        println!(
            "{}",
            terminal::dim("Could not open a browser. Copy the URL above into your browser.")
        );
    }
}

fn dashboard_available(addr: SocketAddr) -> Result<bool, String> {
    let timeout = Duration::from_secs(2);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|error| format!("failed to connect to Dashboard at {addr}: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("failed to configure Dashboard connection: {error}"))?;
    write!(
        stream,
        "GET /dashboard HTTP/1.0\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| format!("failed to request Dashboard: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("failed to read Dashboard response: {error}"))?;
    Ok(response
        .lines()
        .next()
        .is_some_and(|line| line.starts_with("HTTP/1.0 200 ") || line.starts_with("HTTP/1.1 200 ")))
}

async fn run_diagnostics(verbose: bool) -> Result<bool, String> {
    let vars = env::vars().collect();
    let home = user_home()?;
    let runner = ProcessCommandRunner;
    #[cfg(target_os = "linux")]
    let manager = SystemdManager::with_environment_paths(
        &runner,
        pontia_clients::service_path_variables(),
        &[],
    );
    #[cfg(target_os = "macos")]
    let manager = launchd_manager(&runner, &[])?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    return diagnostics::run(&manager, &home, &vars, verbose).await;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    Err("service diagnostics are supported only on Linux and macOS".to_string())
}

fn user_home() -> Result<PathBuf, String> {
    let value = env::var("HOME")
        .map_err(|_| "HOME must be set to locate the per-user service definition".to_string())?;
    let path = PathBuf::from(&value);
    if value.trim().is_empty()
        || !path.is_absolute()
        || path.parent().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("HOME must be a non-root absolute path without parent traversal".to_string());
    }
    Ok(path)
}

fn sibling_pontiad() -> Result<PathBuf, String> {
    let current = env::current_exe()
        .map_err(|error| format!("failed to resolve the pontia executable: {error}"))?
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize the pontia executable: {error}"))?;
    let sibling = current
        .parent()
        .ok_or_else(|| "pontia executable has no parent directory".to_string())?
        .join("pontiad");
    let sibling = sibling.canonicalize().map_err(|error| {
        format!(
            "could not find the sibling pontiad executable at {}: {error}",
            sibling.display()
        )
    })?;
    if !sibling.is_file() || !is_executable(&sibling)? {
        return Err(format!(
            "sibling pontiad is not an executable file: {}",
            sibling.display()
        ));
    }
    Ok(sibling)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> Result<bool, String> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = path
        .metadata()
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    Ok(metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> Result<bool, String> {
    Ok(path.is_file())
}

#[cfg(target_os = "macos")]
fn launchd_manager<'a, R: CommandRunner>(
    runner: &'a R,
    environment_paths: &[(String, PathBuf)],
) -> Result<LaunchdManager<'a, R>, String> {
    Ok(LaunchdManager::with_environment_paths(
        runner,
        current_uid(runner)?,
        pontia_clients::service_path_variables(),
        environment_paths,
        Vec::new(),
    ))
}

#[cfg(target_os = "macos")]
fn current_uid<R: CommandRunner>(runner: &R) -> Result<u32, String> {
    let output = runner.run("id", &["-u".to_string()])?;
    if output.code != 0 {
        return Err(format!("id -u failed: {}", output.stderr.trim()));
    }
    output
        .stdout
        .trim()
        .parse()
        .map_err(|error| format!("id -u returned an invalid user ID: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_tools_are_subcommands() {
        let command = Cli::command();
        for name in AGENT_TOOLS {
            assert!(command.find_subcommand(name).is_some(), "{name}");
        }
    }

    #[test]
    fn cli_command_is_valid() {
        cli_command().debug_assert();
    }

    #[test]
    fn setup_client_names_select_the_registered_integrations() {
        for (name, expected) in [("claude", "claude-agent"), ("codex", "codex"), ("pi", "pi")] {
            let matches = cli_command()
                .try_get_matches_from(["pontia", "setup", name])
                .unwrap();
            let cli = Cli::from_arg_matches(&matches).unwrap();
            assert!(
                matches!(cli.command, Command::Setup { client: Some(client) } if client == expected)
            );
        }
    }

    #[test]
    fn completions_cover_hidden_agent_tools_and_setup_clients() {
        let mut script = Vec::new();
        clap_complete::generate(
            clap_complete::Shell::Bash,
            &mut cli_command(),
            "pontia",
            &mut script,
        );
        let script = String::from_utf8(script).unwrap();
        for word in AGENT_TOOLS.iter().chain(&["claude"]) {
            assert!(script.contains(word), "{word}");
        }
    }

    #[test]
    fn remote_dashboard_url_targets_the_registered_device() {
        assert_eq!(
            remote_dashboard_url("office-mac").unwrap(),
            "https://app.pontia.dev/office-mac"
        );
    }
}
