use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};

use crate::{
    definition::{CODEX_SYSTEMD_SERVICE_NAME, render_codex_systemd},
    lifecycle::DefinitionStore,
    manager::{CommandOutput, CommandRunner},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexSetup {
    pub executable: PathBuf,
    pub home: PathBuf,
    pub username: String,
    pub service_path: PathBuf,
}

pub trait CodexDaemonProbe {
    fn probe(&self, codex_home: &Path) -> Result<(), String>;
}

pub fn inspect<R: CommandRunner>(
    vars: &HashMap<String, String>,
    user_home: &Path,
    runner: &R,
) -> Result<CodexSetup, String> {
    if !cfg!(target_os = "linux") {
        return Err("Codex automatic startup is supported only on Linux with systemd".to_string());
    }

    let executable = resolve_executable(vars)?;
    let home = resolve_codex_home(vars, user_home)?;
    let username = current_username(runner)?;
    let environment = codex_environment(&home)?;
    let program = utf8_path(&executable, "Codex executable")?;

    require_command(
        runner.run_with_env(program, &["--version".to_string()], &environment)?,
        program,
        &["--version"],
        "Codex executable check",
    )?;
    for command in ["start", "version"] {
        let args = [
            "app-server".to_string(),
            "daemon".to_string(),
            command.to_string(),
            "--help".to_string(),
        ];
        require_command(
            runner.run_with_env(program, &args, &environment)?,
            program,
            &["app-server", "daemon", command, "--help"],
            &format!("Codex daemon {command} capability check"),
        )?;
    }

    Ok(CodexSetup {
        executable,
        home,
        username,
        service_path: user_home
            .join(".config/systemd/user")
            .join(CODEX_SYSTEMD_SERVICE_NAME),
    })
}

pub fn initialize<R, S, P>(
    setup: &CodexSetup,
    runner: &R,
    definitions: &S,
    probe: &P,
) -> Result<(), String>
where
    R: CommandRunner,
    S: DefinitionStore,
    P: CodexDaemonProbe,
{
    let definition = render_codex_systemd(&setup.executable, &setup.home)?;
    definitions
        .install(&setup.service_path, &definition)
        .map_err(|error| format!("Codex service definition installation failed: {error}"))?;

    require_systemctl(runner, &["daemon-reload"], "Codex service reload")?;
    require_plain(
        runner,
        "loginctl",
        &["enable-linger", &setup.username],
        "Codex linger enablement",
    )?;
    require_systemctl(
        runner,
        &["enable", "--now", CODEX_SYSTEMD_SERVICE_NAME],
        "Codex service enablement",
    )?;

    let environment = codex_environment(&setup.home)?;
    let program = utf8_path(&setup.executable, "Codex executable")?;
    let version_args = [
        "app-server".to_string(),
        "daemon".to_string(),
        "version".to_string(),
    ];
    let version = runner.run_with_env(program, &version_args, &environment)?;
    require_command(
        version.clone(),
        program,
        &["app-server", "daemon", "version"],
        "Codex daemon version check",
    )?;
    let version: serde_json::Value = serde_json::from_str(&version.stdout)
        .map_err(|error| format!("Codex daemon version check returned invalid JSON: {error}"))?;
    if version["status"] != "running" {
        return Err(format!(
            "Codex daemon version check did not report a running daemon: {}",
            version["status"]
        ));
    }

    let enabled = systemctl(runner, &["is-enabled", CODEX_SYSTEMD_SERVICE_NAME])?;
    require_command(
        enabled.clone(),
        "systemctl",
        &["--user", "is-enabled", CODEX_SYSTEMD_SERVICE_NAME],
        "Codex service enabled-state check",
    )?;
    if enabled.stdout.trim() != "enabled" {
        return Err(format!(
            "Codex service enabled-state check returned {:?}, expected \"enabled\"",
            enabled.stdout.trim()
        ));
    }

    let linger_args = [
        "show-user".to_string(),
        setup.username.clone(),
        "--property=Linger".to_string(),
        "--value".to_string(),
    ];
    let linger = runner.run("loginctl", &linger_args)?;
    require_command(
        linger.clone(),
        "loginctl",
        &["show-user", &setup.username, "--property=Linger", "--value"],
        "Codex linger state check",
    )?;
    if linger.stdout.trim() != "yes" {
        return Err(format!(
            "Codex linger state check returned {:?}, expected \"yes\"",
            linger.stdout.trim()
        ));
    }

    probe
        .probe(&setup.home)
        .map_err(|error| format!("Codex protocol connection check failed: {error}"))
}

fn resolve_executable(vars: &HashMap<String, String>) -> Result<PathBuf, String> {
    let path = vars
        .get("PATH")
        .filter(|path| !path.trim().is_empty())
        .ok_or_else(|| "PATH must be set to locate the Codex executable".to_string())?;
    let current_dir = env::current_dir()
        .map_err(|error| format!("failed to resolve the current directory: {error}"))?;
    for directory in env::split_paths(path) {
        let candidate = if directory.is_absolute() {
            directory.join("codex")
        } else {
            current_dir.join(directory).join("codex")
        };
        if candidate.is_file() && is_executable(&candidate)? {
            return Ok(candidate);
        }
    }
    Err("Codex must be installed and executable on PATH".to_string())
}

fn resolve_codex_home(vars: &HashMap<String, String>, user_home: &Path) -> Result<PathBuf, String> {
    let configured = vars
        .get("CODEX_HOME")
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| user_home.join(".codex"));
    if !configured.is_absolute() {
        return Err(format!(
            "CODEX_HOME must be an absolute path: {}",
            configured.display()
        ));
    }
    match configured.metadata() {
        Ok(metadata) if !metadata.is_dir() => Err(format!(
            "CODEX_HOME must be a directory: {}",
            configured.display()
        )),
        Ok(_) => configured.canonicalize().map_err(|error| {
            format!(
                "failed to resolve CODEX_HOME {}: {error}",
                configured.display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(configured),
        Err(error) => Err(format!(
            "failed to inspect CODEX_HOME {}: {error}",
            configured.display()
        )),
    }
}

fn current_username<R: CommandRunner>(runner: &R) -> Result<String, String> {
    let output = runner.run("id", &["-un".to_string()])?;
    require_command(output.clone(), "id", &["-un"], "current username check")?;
    let username = output.stdout.trim();
    if username.is_empty() || username.chars().any(char::is_whitespace) {
        return Err("id -un returned an invalid username".to_string());
    }
    Ok(username.to_string())
}

fn codex_environment(home: &Path) -> Result<Vec<(String, String)>, String> {
    Ok(vec![(
        "CODEX_HOME".to_string(),
        utf8_path(home, "CODEX_HOME")?.to_string(),
    )])
}

fn require_systemctl<R: CommandRunner>(
    runner: &R,
    args: &[&str],
    step: &str,
) -> Result<(), String> {
    let output = systemctl(runner, args)?;
    let display_args = std::iter::once("--user")
        .chain(args.iter().copied())
        .collect::<Vec<_>>();
    require_command(output, "systemctl", &display_args, step)
}

fn systemctl<R: CommandRunner>(runner: &R, args: &[&str]) -> Result<CommandOutput, String> {
    runner.run(
        "systemctl",
        &std::iter::once("--user")
            .chain(args.iter().copied())
            .map(String::from)
            .collect::<Vec<_>>(),
    )
}

fn require_plain<R: CommandRunner>(
    runner: &R,
    program: &str,
    args: &[&str],
    step: &str,
) -> Result<(), String> {
    let owned = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    require_command(runner.run(program, &owned)?, program, args, step)
}

fn require_command(
    output: CommandOutput,
    program: &str,
    args: &[&str],
    step: &str,
) -> Result<(), String> {
    if output.code == 0 {
        return Ok(());
    }
    let detail = if output.stderr.trim().is_empty() {
        output.stdout.trim()
    } else {
        output.stderr.trim()
    };
    Err(format!(
        "{step} failed: {program} {} exited with code {}: {detail}",
        args.join(" "),
        output.code
    ))
}

fn utf8_path<'a>(path: &'a Path, description: &str) -> Result<&'a str, String> {
    path.to_str()
        .ok_or_else(|| format!("{description} is not valid UTF-8: {}", path.display()))
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
