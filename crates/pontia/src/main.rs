mod login;
mod remote;
mod update;
mod workflow;

use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Component, Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode, Stdio},
    time::Duration,
};

use clap::{Parser, Subcommand};
use dialoguer::Confirm;
use pontia::{
    init::{self, InitPlatform},
    lifecycle::{EnabledState, Lifecycle, LifecycleStatus, RunState, ServiceManager, UpOptions},
    manager::ProcessCommandRunner,
    runtime_io::{FileDefinitionStore, HttpHealthProbe},
};
use pontia_application::client_contract::ClientIntegration;
use pontia_config::AppConfig;
#[cfg(target_os = "macos")]
use pontia_runtime::local_service::launchd_executable_search_path;
use pontia_runtime::local_service::{CommandRunner, DefinitionStore};
use std::sync::Arc;

#[cfg(target_os = "macos")]
use pontia::manager::LaunchdManager;
#[cfg(target_os = "linux")]
use pontia::manager::SystemdManager;

#[derive(Debug, Parser)]
#[command(
    name = "pontia",
    version = pontia_version::version(),
    about = "Control Pontia from the command line"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Configure Pontia interactively and start its per-user service
    Init,
    /// Sign in to the Pontia cloud from a headless terminal
    Login,
    /// Configure remote device access
    Remote(remote::RemoteCommand),
    /// Install and start the per-user Pontia service
    Up,
    /// Update pontia and pontiad to the latest stable release
    Update,
    /// Stop and disable the per-user Pontia service
    Down,
    /// Show the Pontia service and health state
    Status,
    /// Run and interact with Workflows
    Workflow(workflow::WorkflowCommand),
}

#[derive(Debug)]
enum LifecycleCommand {
    Up,
    Down,
    Status,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match execute(cli.command).await {
        Ok(operational) if operational => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("pontia: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn execute(command: Command) -> Result<bool, String> {
    match command {
        Command::Init => run_init().await,
        Command::Login => {
            let vars: HashMap<String, String> = env::vars().collect();
            login::run(&vars).await?;
            Ok(true)
        }
        Command::Workflow(command) => {
            let config = pontia_clients::config_from_env().map_err(|error| error.to_string())?;
            workflow::run(command, &config).await?;
            Ok(true)
        }
        Command::Remote(command) => {
            let vars: HashMap<String, String> = env::vars().collect();
            remote::run(command, &vars).await?;
            restart_service_for_remote_config()?;
            Ok(true)
        }
        Command::Update => run_update().await,
        Command::Up => run_lifecycle(LifecycleCommand::Up),
        Command::Down => run_lifecycle(LifecycleCommand::Down),
        Command::Status => run_lifecycle(LifecycleCommand::Status),
    }
}

async fn run_update() -> Result<bool, String> {
    let update = update::prepare().await?;
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
            let config = update_service_config(&manager, &user_home()?, false)?;
            if config.is_some() && manager.running_executable()? != update.daemon_path() {
                return Err("the running service belongs to another Pontia installation; run its sibling pontia update instead".into());
            }
            config
        } else {
            None
        };
        if config.is_none() {
            update.ensure_no_unmanaged_daemon()?;
        }
        update.install(|| {
            if let Some(config) = &config {
                eprintln!("Restarting Pontia and waiting for it to become healthy...");
                Lifecycle::new(&manager, &FileDefinitionStore, &HttpHealthProbe).restart(config)?;
            }
            Ok(())
        })?;
        Ok(true)
    }
    #[cfg(target_os = "macos")]
    {
        let runner = ProcessCommandRunner;
        let manager = launchd_manager(&runner, &[])?;
        // A loaded KeepAlive job can restart while its process is temporarily absent,
        // so keep treating stopped or failed loaded jobs as managed during replacement.
        let config = update_service_config(&manager, &user_home()?, true)?;
        if config.is_some() && manager.running_executable()? != update.daemon_path() {
            return Err("the running service belongs to another Pontia installation; run its sibling pontia update instead".into());
        }
        if config.is_none() {
            update.ensure_no_unmanaged_daemon()?;
        }
        update.install(|| {
            if let Some(config) = &config {
                eprintln!("Restarting Pontia and waiting for it to become healthy...");
                Lifecycle::new(&manager, &FileDefinitionStore, &HttpHealthProbe).restart(config)?;
            }
            Ok(())
        })?;
        Ok(true)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = update;
        Err("pontia update supports only Linux and macOS".into())
    }
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

fn run_lifecycle(command: LifecycleCommand) -> Result<bool, String> {
    service_manager_preflight()?;
    let runner = ProcessCommandRunner;

    #[cfg(target_os = "linux")]
    {
        run_with_manager(
            command,
            &SystemdManager::with_environment_paths(
                &runner,
                pontia_clients::service_path_variables(),
                &[],
            ),
        )
    }

    #[cfg(target_os = "macos")]
    {
        let manager = launchd_manager(&runner, &[])?;
        run_with_manager(command, &manager)
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

fn run_with_manager<M: ServiceManager>(
    command: LifecycleCommand,
    manager: &M,
) -> Result<bool, String> {
    let definitions = FileDefinitionStore;
    let health = HttpHealthProbe;
    let lifecycle = Lifecycle::new(manager, &definitions, &health);

    match command {
        LifecycleCommand::Up => {
            let config = pontia_clients::config_from_env().map_err(|error| error.to_string())?;
            eprintln!("Starting Pontia service and waiting for it to become healthy...");
            start_with_lifecycle(
                &lifecycle,
                &config,
                UpOptions {
                    restart_running: true,
                },
            )?;
            println!("Pontia is up and healthy.");
            Ok(true)
        }
        LifecycleCommand::Down => {
            lifecycle.down()?;
            println!("Pontia is down.");
            Ok(true)
        }
        LifecycleCommand::Status => {
            let status = lifecycle.status(&user_home()?)?;
            print_status(&status);
            Ok(status.is_operational())
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

impl InitPlatform for RealInitPlatform {
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
        ),
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
        &launchd_manager(&runner, environment_paths)?,
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

    let enable_remote = Confirm::new()
        .with_prompt("Sign in and enable remote access?")
        .default(true)
        .interact()
        .map_err(|error| format!("failed to read the remote access selection: {error}"))?;

    if !enable_remote {
        println!(
            "Remote access skipped. Run `pontia login` and `pontia remote enable` later to enable it."
        );
        open_dashboard("Local Dashboard", &outcome.local_dashboard_url);
        return Ok(true);
    }

    match enable_remote_access(&vars).await {
        Ok(url) => {
            open_dashboard("Remote Dashboard", &url);
            Ok(true)
        }
        Err(error) => {
            println!(
                "Remote access setup did not complete. Local initialization is complete; retry with `pontia login` and `pontia remote enable`."
            );
            open_dashboard("Local Dashboard", &outcome.local_dashboard_url);
            Err(error)
        }
    }
}

async fn enable_remote_access(vars: &HashMap<String, String>) -> Result<String, String> {
    if !login::has_valid_credential(vars)? {
        login::run_with_verification(vars, |url| {
            let _ = open_browser(url);
        })
        .await?;
    } else {
        println!("Using the existing Pontia cloud login.");
    }
    let access = remote::enable(vars).await?;
    restart_service_for_remote_config()?;
    remote_dashboard_url(&access.device_handle)
}

fn remote_dashboard_url(device_handle: &str) -> Result<String, String> {
    let mut url = url::Url::parse("https://app.pontia.dev/")
        .map_err(|error| format!("failed to build the remote Dashboard URL: {error}"))?;
    url.path_segments_mut()
        .map_err(|_| "failed to build the remote Dashboard URL".to_string())?
        .push(device_handle);
    Ok(url.to_string())
}

fn open_dashboard(label: &str, url: &str) {
    println!("{label}:\n\n{url}\n");
    if open_browser(url).is_ok() {
        println!("✓ Dashboard opened. `pontia down` stops the local service.");
    } else {
        println!("Browser not opened; use the URL above. `pontia down` stops the local service.");
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

fn print_status(status: &LifecycleStatus) {
    println!(
        "definition: {}",
        if status.definition_installed {
            "installed"
        } else {
            "missing"
        }
    );
    println!(
        "enabled: {}",
        match status.service.enabled {
            EnabledState::Enabled => "enabled",
            EnabledState::Disabled => "disabled",
            EnabledState::Unknown => "unknown",
        }
    );
    println!(
        "state: {}",
        match status.service.run_state {
            RunState::Running => "running",
            RunState::Stopped => "stopped",
            RunState::Starting => "starting",
            RunState::Failed => "failed",
        }
    );
    println!(
        "http: {}",
        if status.http_healthy {
            "healthy"
        } else {
            "unhealthy"
        }
    );
    println!(
        "PONTIA_HOME: {}",
        status
            .persisted_home
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_string())
    );
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
        launchd_executable_search_path(env::var_os("PATH").as_deref(), &user_home()?, &[])?,
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
    fn remote_dashboard_url_targets_the_registered_device() {
        assert_eq!(
            remote_dashboard_url("office-mac").unwrap(),
            "https://app.pontia.dev/office-mac"
        );
    }
}
