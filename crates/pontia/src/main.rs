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
use pontia::{
    init::{self, InitPlatform},
    lifecycle::{EnabledState, Lifecycle, LifecycleStatus, RunState, ServiceManager, UpOptions},
    manager::ProcessCommandRunner,
    runtime_io::{FileDefinitionStore, HttpHealthProbe},
};
use pontia_config::AppConfig;

#[cfg(target_os = "linux")]
use pontia::manager::SystemdManager;
#[cfg(target_os = "macos")]
use pontia::manager::{CommandRunner, LaunchdManager};

#[derive(Debug, Parser)]
#[command(
    name = "pontia",
    version,
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
    /// Install and start the per-user Pontia service
    Up,
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
        Command::Init => run_init(),
        Command::Workflow(command) => {
            let config = AppConfig::from_env().map_err(|error| error.to_string())?;
            workflow::run(command, &config).await?;
            Ok(true)
        }
        Command::Up => run_lifecycle(LifecycleCommand::Up),
        Command::Down => run_lifecycle(LifecycleCommand::Down),
        Command::Status => run_lifecycle(LifecycleCommand::Status),
    }
}

fn run_lifecycle(command: LifecycleCommand) -> Result<bool, String> {
    service_manager_preflight()?;
    let runner = ProcessCommandRunner;

    #[cfg(target_os = "linux")]
    {
        run_with_manager(command, &SystemdManager::new(&runner))
    }

    #[cfg(target_os = "macos")]
    {
        let uid = current_uid(&runner)?;
        run_with_manager(command, &LaunchdManager::new(&runner, uid))
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
            let config = AppConfig::from_env().map_err(|error| error.to_string())?;
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
    fn preflight(&self, install_pi: bool) -> Result<(), String> {
        service_manager_preflight()?;
        sibling_pontiad()?;
        if install_pi {
            let output = ProcessCommand::new("pi")
                .arg("--version")
                .output()
                .map_err(|error| format!("pi must be installed and executable: {error}"))?;
            if !output.status.success() {
                return Err(format!("pi --version failed with {}", output.status));
            }
        }
        Ok(())
    }

    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), String> {
        getrandom::fill(bytes)
            .map_err(|error| format!("failed to generate a secure token: {error}"))
    }

    fn install_pi(&self) -> Result<(), String> {
        let status = ProcessCommand::new("pi")
            .args(["install", "npm:@pontia/pi-client-plugin"])
            .status()
            .map_err(|error| format!("failed to run pi install: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "pi install npm:@pontia/pi-client-plugin failed with {status}"
            ))
        }
    }

    fn start_service(&self, config: &AppConfig, config_changed: bool) -> Result<(), String> {
        service_manager_preflight()?;
        start_init_service(config, config_changed)
    }

    fn dashboard_available(&self, addr: SocketAddr) -> Result<bool, String> {
        dashboard_available(addr)
    }

    fn open_browser(&self, url: &str) -> Result<(), String> {
        open_browser(url)
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
fn start_init_service(config: &AppConfig, config_changed: bool) -> Result<(), String> {
    let runner = ProcessCommandRunner;
    start_init_with_manager(&SystemdManager::new(&runner), config, config_changed)
}

#[cfg(target_os = "macos")]
fn start_init_service(config: &AppConfig, config_changed: bool) -> Result<(), String> {
    let runner = ProcessCommandRunner;
    start_init_with_manager(
        &LaunchdManager::new(&runner, current_uid(&runner)?),
        config,
        config_changed,
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn start_init_service(_config: &AppConfig, _config_changed: bool) -> Result<(), String> {
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

fn run_init() -> Result<bool, String> {
    let vars: HashMap<String, String> = env::vars().collect();
    init::run_interactive(&vars, &RealInitPlatform)?;
    Ok(true)
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
