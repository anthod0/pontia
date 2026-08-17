mod workflow;

use std::{
    env,
    path::{Component, Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use pontia::{
    lifecycle::{EnabledState, Lifecycle, LifecycleStatus, RunState, ServiceManager},
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
    let runner = ProcessCommandRunner;

    #[cfg(target_os = "linux")]
    {
        if !Path::new("/run/systemd/system").is_dir() {
            return Err(
                "automatic lifecycle management requires a running systemd user service manager; supervise pontiad with OpenRC, runit, or s6 on this Linux system"
                    .to_string(),
            );
        }
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
            let user_home = user_home()?;
            let pontiad = sibling_pontiad()?;
            lifecycle.up(&config, &pontiad, &user_home)?;
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
