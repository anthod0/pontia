use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use pontia_application::client_contract::{ClientIntegration, PreparedClientIntegration};
use pontia_runtime::local_service::{
    CommandOutput, CommandRunner, DefinitionStore, absolute_utf8_path,
    launchd_executable_search_path, systemd_quote, xml_escape,
};

pub const CODEX_SYSTEMD_SERVICE_NAME: &str = "pontia-codex.service";
pub const CODEX_LAUNCHD_LABEL: &str = "dev.pontia.codex-daemon";

pub fn integration() -> std::sync::Arc<dyn ClientIntegration> {
    std::sync::Arc::new(CodexIntegration)
}

struct CodexIntegration;

impl ClientIntegration for CodexIntegration {
    fn client_type(&self) -> &'static str {
        "codex"
    }
    fn selected_by_default(&self) -> bool {
        false
    }
    fn skipped_summary(&self) -> Vec<String> {
        vec!["Codex integration: skip".into()]
    }
    fn service_path_variables(&self) -> &'static [&'static str] {
        &["CODEX_HOME"]
    }
    fn prepare(
        &self,
        vars: &HashMap<String, String>,
        user_home: &Path,
        runner: &dyn CommandRunner,
    ) -> Result<Box<dyn PreparedClientIntegration>, String> {
        Ok(Box::new(inspect(vars, user_home, runner)?))
    }
}

impl PreparedClientIntegration for CodexSetup {
    fn summary(&self) -> Vec<String> {
        let summary = vec![
            "Codex integration: register autostart".into(),
            format!("Codex executable: {}", self.executable.display()),
            format!("CODEX_HOME: {}", self.home.display()),
            format!("Codex service: {}", self.service_path.display()),
        ];
        #[cfg(target_os = "linux")]
        let summary = {
            let mut summary = summary;
            if let ServiceUserIdentity::SystemdUsername(username) = &self.service_user {
                summary.push(format!("user linger: enable for {username}"));
            }
            summary
        };
        summary
    }
    fn preflight(&self, _runner: &dyn CommandRunner) -> Result<(), String> {
        Ok(())
    }
    fn install(
        &self,
        runner: &dyn CommandRunner,
        definitions: &dyn DefinitionStore,
    ) -> Result<(), String> {
        initialize(self, runner, definitions, &DaemonProbe)
    }
    fn completion(&self) -> &'static str {
        "Configured Codex autostart and control connection"
    }
    fn service_environment_paths(&self) -> Vec<(String, PathBuf)> {
        vec![("CODEX_HOME".into(), self.home.clone())]
    }
}

struct DaemonProbe;

impl CodexDaemonProbe for DaemonProbe {
    fn probe(&self, home: &Path) -> Result<(), String> {
        let home = home.to_path_buf();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .map_err(|error| format!("failed to create Codex probe runtime: {error}"))?
                .block_on(crate::runtime::probe_daemon(&home))
                .map_err(|error| error.to_string())
        })
        .join()
        .map_err(|_| "Codex daemon probe panicked".to_string())?
    }
}

pub fn render_codex_systemd(codex: &Path, codex_home: &Path) -> Result<String, String> {
    let codex = absolute_utf8_path(codex, "Codex executable")?;
    let codex_home = absolute_utf8_path(codex_home, "CODEX_HOME")?;
    Ok(format!(
        "[Unit]\nDescription=Codex App Server Daemon Startup\nAfter=network.target\n\n[Service]\nType=oneshot\nExecStart=\"{}\" app-server daemon start\nEnvironment=\"CODEX_HOME={}\"\n\n[Install]\nWantedBy=default.target\n",
        systemd_quote(codex),
        systemd_quote(codex_home),
    ))
}

pub fn render_codex_launchd(codex: &Path, codex_home: &Path) -> Result<String, String> {
    let search_path = codex
        .parent()
        .into_iter()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    render_codex_launchd_with_search_path(codex, codex_home, &search_path)
}

fn render_codex_launchd_with_search_path(
    codex: &Path,
    codex_home: &Path,
    executable_search_path: &[PathBuf],
) -> Result<String, String> {
    let codex = xml_escape(absolute_utf8_path(codex, "Codex executable")?)?;
    let codex_home = xml_escape(absolute_utf8_path(codex_home, "CODEX_HOME")?)?;
    for path in executable_search_path {
        absolute_utf8_path(path, "Codex executable search directory")?;
    }
    let search_path = std::env::join_paths(executable_search_path)
        .map_err(|error| format!("Codex executable search path cannot be represented: {error}"))?;
    let search_path = xml_escape(
        search_path
            .to_str()
            .ok_or_else(|| "Codex executable search path is not valid UTF-8".to_string())?,
    )?;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{CODEX_LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{codex}</string>
    <string>app-server</string>
    <string>daemon</string>
    <string>start</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>CODEX_HOME</key>
    <string>{codex_home}</string>
    <key>PATH</key>
    <string>{search_path}</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceUserIdentity {
    SystemdUsername(String),
    LaunchdUid(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexSetup {
    pub executable: PathBuf,
    pub home: PathBuf,
    pub service_user: ServiceUserIdentity,
    pub service_path: PathBuf,
    pub executable_search_path: Vec<PathBuf>,
}

impl CodexSetup {
    #[cfg(target_os = "linux")]
    fn systemd_username(&self) -> Result<&str, String> {
        match &self.service_user {
            ServiceUserIdentity::SystemdUsername(username) => Ok(username),
            ServiceUserIdentity::LaunchdUid(_) => {
                Err("Codex setup does not contain a systemd username".into())
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn launchd_uid(&self) -> Result<u32, String> {
        match &self.service_user {
            ServiceUserIdentity::LaunchdUid(uid) => Ok(*uid),
            ServiceUserIdentity::SystemdUsername(_) => {
                Err("Codex setup does not contain a launchd user ID".into())
            }
        }
    }
}

pub trait CodexDaemonProbe {
    fn probe(&self, codex_home: &Path) -> Result<(), String>;
}

pub fn inspect<R: CommandRunner + ?Sized>(
    vars: &HashMap<String, String>,
    user_home: &Path,
    runner: &R,
) -> Result<CodexSetup, String> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err("Codex automatic startup is supported only on Linux and macOS".to_string());
    }

    let executable = resolve_executable(vars)?;
    let home = resolve_codex_home(vars, user_home)?;
    let service_user = current_service_user(runner)?;
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

    let preferred_paths = executable
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect::<Vec<_>>();
    let executable_search_path = launchd_executable_search_path(
        vars.get("PATH").map(|path| OsStr::new(path.as_str())),
        user_home,
        &preferred_paths,
    )?;
    Ok(CodexSetup {
        executable,
        home,
        service_user,
        service_path: codex_service_path(user_home),
        executable_search_path,
    })
}

pub fn initialize<R, S, P>(
    setup: &CodexSetup,
    runner: &R,
    definitions: &S,
    probe: &P,
) -> Result<(), String>
where
    R: CommandRunner + ?Sized,
    S: DefinitionStore + ?Sized,
    P: CodexDaemonProbe + ?Sized,
{
    install_and_start_service(setup, runner, definitions)?;

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

    verify_service(setup, runner)?;

    probe
        .probe(&setup.home)
        .map_err(|error| format!("Codex protocol connection check failed: {error}"))
}

#[cfg(target_os = "linux")]
fn codex_service_path(user_home: &Path) -> PathBuf {
    user_home
        .join(".config/systemd/user")
        .join(CODEX_SYSTEMD_SERVICE_NAME)
}

#[cfg(target_os = "macos")]
fn codex_service_path(user_home: &Path) -> PathBuf {
    user_home
        .join("Library/LaunchAgents")
        .join(format!("{CODEX_LAUNCHD_LABEL}.plist"))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn codex_service_path(user_home: &Path) -> PathBuf {
    user_home.join("pontia-codex.unsupported")
}

#[cfg(target_os = "linux")]
fn install_and_start_service<R, S>(
    setup: &CodexSetup,
    runner: &R,
    definitions: &S,
) -> Result<(), String>
where
    R: CommandRunner + ?Sized,
    S: DefinitionStore + ?Sized,
{
    let definition = render_codex_systemd(&setup.executable, &setup.home)?;
    definitions
        .install(&setup.service_path, &definition)
        .map_err(|error| format!("Codex service definition installation failed: {error}"))?;
    require_systemctl(runner, &["daemon-reload"], "Codex service reload")?;
    require_plain(
        runner,
        "loginctl",
        &["enable-linger", setup.systemd_username()?],
        "Codex linger enablement",
    )?;
    require_systemctl(
        runner,
        &["enable", "--now", CODEX_SYSTEMD_SERVICE_NAME],
        "Codex service enablement",
    )
}

#[cfg(target_os = "macos")]
fn install_and_start_service<R, S>(
    setup: &CodexSetup,
    runner: &R,
    definitions: &S,
) -> Result<(), String>
where
    R: CommandRunner + ?Sized,
    S: DefinitionStore + ?Sized,
{
    let definition = render_codex_launchd_with_search_path(
        &setup.executable,
        &setup.home,
        &setup.executable_search_path,
    )?;
    definitions
        .install(&setup.service_path, &definition)
        .map_err(|error| format!("Codex service definition installation failed: {error}"))?;
    let domain = format!("gui/{}", setup.launchd_uid()?);
    let target = format!("{domain}/{CODEX_LAUNCHD_LABEL}");
    let bootout_args = ["bootout".to_string(), target.clone()];
    let bootout = runner.run("launchctl", &bootout_args)?;
    if bootout.code != 0 && !launchctl_service_missing(&bootout) {
        return require_command(
            bootout,
            "launchctl",
            &["bootout", &target],
            "Codex service unload",
        );
    }
    require_plain(
        runner,
        "launchctl",
        &["enable", &target],
        "Codex service enablement",
    )?;
    let service_path = utf8_path(&setup.service_path, "Codex service path")?;
    require_plain(
        runner,
        "launchctl",
        &["bootstrap", &domain, service_path],
        "Codex service load",
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn install_and_start_service<R, S>(
    _setup: &CodexSetup,
    _runner: &R,
    _definitions: &S,
) -> Result<(), String>
where
    R: CommandRunner + ?Sized,
    S: DefinitionStore + ?Sized,
{
    Err("Codex automatic startup is supported only on Linux and macOS".into())
}

#[cfg(target_os = "linux")]
fn verify_service<R: CommandRunner + ?Sized>(setup: &CodexSetup, runner: &R) -> Result<(), String> {
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
        setup.systemd_username()?.to_string(),
        "--property=Linger".to_string(),
        "--value".to_string(),
    ];
    let linger = runner.run("loginctl", &linger_args)?;
    require_command(
        linger.clone(),
        "loginctl",
        &[
            "show-user",
            setup.systemd_username()?,
            "--property=Linger",
            "--value",
        ],
        "Codex linger state check",
    )?;
    if linger.stdout.trim() != "yes" {
        return Err(format!(
            "Codex linger state check returned {:?}, expected \"yes\"",
            linger.stdout.trim()
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn verify_service<R: CommandRunner + ?Sized>(setup: &CodexSetup, runner: &R) -> Result<(), String> {
    let domain = format!("gui/{}", setup.launchd_uid()?);
    let target = format!("{domain}/{CODEX_LAUNCHD_LABEL}");
    require_plain(
        runner,
        "launchctl",
        &["print", &target],
        "Codex service loaded-state check",
    )?;
    let args = ["print-disabled".to_string(), domain.clone()];
    let disabled = runner.run("launchctl", &args)?;
    require_command(
        disabled.clone(),
        "launchctl",
        &["print-disabled", &domain],
        "Codex service enabled-state check",
    )?;
    if disabled.stdout.lines().any(|line| {
        line.contains(&format!("\"{CODEX_LAUNCHD_LABEL}\"")) && line.contains("=> true")
    }) {
        return Err("Codex service is disabled in the launchd user domain".into());
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn verify_service<R: CommandRunner + ?Sized>(
    _setup: &CodexSetup,
    _runner: &R,
) -> Result<(), String> {
    Err("Codex automatic startup is supported only on Linux and macOS".into())
}

#[cfg(target_os = "macos")]
fn launchctl_service_missing(output: &CommandOutput) -> bool {
    let text = format!("{}\n{}", output.stdout, output.stderr).to_ascii_lowercase();
    text.contains("could not find service")
        || text.contains("no such process")
        || text.contains("does not exist")
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

#[cfg(target_os = "linux")]
fn current_service_user<R: CommandRunner + ?Sized>(
    runner: &R,
) -> Result<ServiceUserIdentity, String> {
    let output = runner.run("id", &["-un".to_string()])?;
    require_command(output.clone(), "id", &["-un"], "current username check")?;
    let username = output.stdout.trim();
    if username.is_empty() || username.chars().any(char::is_whitespace) {
        return Err("id -un returned an invalid username".to_string());
    }
    Ok(ServiceUserIdentity::SystemdUsername(username.to_string()))
}

#[cfg(target_os = "macos")]
fn current_service_user<R: CommandRunner + ?Sized>(
    runner: &R,
) -> Result<ServiceUserIdentity, String> {
    let output = runner.run("id", &["-u".to_string()])?;
    require_command(output.clone(), "id", &["-u"], "current user ID check")?;
    output
        .stdout
        .trim()
        .parse::<u32>()
        .map(ServiceUserIdentity::LaunchdUid)
        .map_err(|_| "id -u returned an invalid user ID".to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn current_service_user<R: CommandRunner + ?Sized>(
    _runner: &R,
) -> Result<ServiceUserIdentity, String> {
    Err("Codex automatic startup is supported only on Linux and macOS".into())
}

fn codex_environment(home: &Path) -> Result<Vec<(String, String)>, String> {
    Ok(vec![(
        "CODEX_HOME".to_string(),
        utf8_path(home, "CODEX_HOME")?.to_string(),
    )])
}

#[cfg(target_os = "linux")]
fn require_systemctl<R: CommandRunner + ?Sized>(
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

#[cfg(target_os = "linux")]
fn systemctl<R: CommandRunner + ?Sized>(
    runner: &R,
    args: &[&str],
) -> Result<CommandOutput, String> {
    runner.run(
        "systemctl",
        &std::iter::once("--user")
            .chain(args.iter().copied())
            .map(String::from)
            .collect::<Vec<_>>(),
    )
}

fn require_plain<R: CommandRunner + ?Sized>(
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
