use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    definition::{LAUNCHD_LABEL, SYSTEMD_SERVICE_NAME, render_launchd, render_systemd},
    lifecycle::{EnabledState, RunState, ServiceManager, ServiceStatus},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait CommandRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String>;
}

#[derive(Debug, Default)]
pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|error| format!("failed to execute {program}: {error}"))?;
        Ok(CommandOutput {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8(output.stdout)
                .map_err(|_| format!("{program} stdout is not valid UTF-8"))?,
            stderr: String::from_utf8(output.stderr)
                .map_err(|_| format!("{program} stderr is not valid UTF-8"))?,
        })
    }
}

pub struct SystemdManager<'a, R> {
    runner: &'a R,
}

impl<'a, R: CommandRunner> SystemdManager<'a, R> {
    pub fn new(runner: &'a R) -> Self {
        Self { runner }
    }

    fn systemctl(&self, args: &[&str]) -> Result<CommandOutput, String> {
        self.runner.run(
            "systemctl",
            &std::iter::once("--user")
                .chain(args.iter().copied())
                .map(String::from)
                .collect::<Vec<_>>(),
        )
    }

    fn require_systemctl(&self, args: &[&str]) -> Result<(), String> {
        let output = self.systemctl(args)?;
        if output.code == 0 {
            Ok(())
        } else {
            Err(command_failure("systemctl", args, &output))
        }
    }
}

impl<R: CommandRunner> ServiceManager for SystemdManager<'_, R> {
    fn definition_path(&self, user_home: &Path) -> PathBuf {
        user_home.join(".config/systemd/user/pontia.service")
    }

    fn render_definition(&self, pontiad: &Path, pontia_home: &Path) -> Result<String, String> {
        render_systemd(pontiad, pontia_home)
    }

    fn persisted_home(&self, definition: &str) -> Result<PathBuf, String> {
        parse_systemd_home(definition)
    }

    fn status(&self) -> Result<ServiceStatus, String> {
        let enabled_output = self.systemctl(&["is-enabled", SYSTEMD_SERVICE_NAME])?;
        let enabled_text = enabled_output.stdout.trim();
        let enabled = match enabled_text {
            "enabled" | "enabled-runtime" if enabled_output.code == 0 => EnabledState::Enabled,
            "disabled" | "masked" | "masked-runtime" | "not-found" => EnabledState::Disabled,
            _ if enabled_output.code != 0 && is_missing(&enabled_output) => EnabledState::Disabled,
            _ if enabled_output.code != 0 => {
                return Err(command_failure(
                    "systemctl",
                    &["is-enabled", SYSTEMD_SERVICE_NAME],
                    &enabled_output,
                ));
            }
            _ => EnabledState::Unknown,
        };

        let show_args = [
            "show",
            SYSTEMD_SERVICE_NAME,
            "--property=LoadState",
            "--property=ActiveState",
        ];
        let show = self.systemctl(&show_args)?;
        if show.code != 0 && !is_missing(&show) {
            return Err(command_failure("systemctl", &show_args, &show));
        }
        let load_state = property(&show.stdout, "LoadState").unwrap_or("not-found");
        let active_state = property(&show.stdout, "ActiveState").unwrap_or("inactive");
        let loaded = load_state == "loaded";
        let run_state = match active_state {
            "active" => RunState::Running,
            "activating" | "reloading" | "deactivating" => RunState::Starting,
            "failed" => RunState::Failed,
            _ => RunState::Stopped,
        };
        Ok(ServiceStatus {
            enabled,
            loaded,
            run_state,
        })
    }

    fn apply(
        &self,
        _definition_path: &Path,
        _definition_changed: bool,
        restart_running: bool,
        _previous: ServiceStatus,
    ) -> Result<(), String> {
        self.require_systemctl(&["daemon-reload"])?;
        self.require_systemctl(&["enable", "--now", SYSTEMD_SERVICE_NAME])?;
        if restart_running {
            self.require_systemctl(&["restart", SYSTEMD_SERVICE_NAME])?;
        }
        Ok(())
    }

    fn down(&self) -> Result<(), String> {
        let args = ["disable", "--now", SYSTEMD_SERVICE_NAME];
        let output = self.systemctl(&args)?;
        if output.code == 0 || is_missing(&output) {
            Ok(())
        } else {
            Err(command_failure("systemctl", &args, &output))
        }
    }
}

pub struct LaunchdManager<'a, R> {
    runner: &'a R,
    uid: u32,
}

impl<'a, R: CommandRunner> LaunchdManager<'a, R> {
    pub fn new(runner: &'a R, uid: u32) -> Self {
        Self { runner, uid }
    }

    fn domain(&self) -> String {
        format!("gui/{}", self.uid)
    }

    fn target(&self) -> String {
        format!("{}/{LAUNCHD_LABEL}", self.domain())
    }

    fn launchctl(&self, args: Vec<String>) -> Result<CommandOutput, String> {
        self.runner.run("launchctl", &args)
    }

    fn require_launchctl(&self, args: Vec<String>) -> Result<(), String> {
        let output = self.launchctl(args.clone())?;
        if output.code == 0 {
            Ok(())
        } else {
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            Err(command_failure("launchctl", &refs, &output))
        }
    }
}

impl<R: CommandRunner> ServiceManager for LaunchdManager<'_, R> {
    fn definition_path(&self, user_home: &Path) -> PathBuf {
        user_home.join("Library/LaunchAgents/dev.pontia.pontiad.plist")
    }

    fn render_definition(&self, pontiad: &Path, pontia_home: &Path) -> Result<String, String> {
        render_launchd(pontiad, pontia_home)
    }

    fn persisted_home(&self, definition: &str) -> Result<PathBuf, String> {
        parse_launchd_home(definition)
    }

    fn status(&self) -> Result<ServiceStatus, String> {
        let domain = self.domain();
        let disabled_args = vec!["print-disabled".to_string(), domain];
        let disabled = self.launchctl(disabled_args.clone())?;
        if disabled.code != 0 {
            let refs = disabled_args.iter().map(String::as_str).collect::<Vec<_>>();
            return Err(command_failure("launchctl", &refs, &disabled));
        }
        let is_disabled = disabled
            .stdout
            .lines()
            .any(|line| line.contains(&format!("\"{LAUNCHD_LABEL}\"")) && line.contains("=> true"));

        let target = self.target();
        let print_args = vec!["print".to_string(), target];
        let printed = self.launchctl(print_args.clone())?;
        if printed.code != 0 {
            if is_missing(&printed) {
                return Ok(ServiceStatus {
                    enabled: if is_disabled {
                        EnabledState::Disabled
                    } else {
                        EnabledState::Enabled
                    },
                    loaded: false,
                    run_state: RunState::Stopped,
                });
            }
            let refs = print_args.iter().map(String::as_str).collect::<Vec<_>>();
            return Err(command_failure("launchctl", &refs, &printed));
        }

        let state = property_with_spaces(&printed.stdout, "state");
        let run_state = match state {
            Some("running") => RunState::Running,
            Some("waiting") => RunState::Starting,
            Some("exited")
                if property_with_spaces(&printed.stdout, "last exit code") == Some("0") =>
            {
                RunState::Stopped
            }
            Some("exited") => RunState::Failed,
            _ => RunState::Starting,
        };
        Ok(ServiceStatus {
            enabled: if is_disabled {
                EnabledState::Disabled
            } else {
                EnabledState::Enabled
            },
            loaded: true,
            run_state,
        })
    }

    fn apply(
        &self,
        definition_path: &Path,
        definition_changed: bool,
        restart_running: bool,
        previous: ServiceStatus,
    ) -> Result<(), String> {
        let target = self.target();
        self.require_launchctl(vec!["enable".to_string(), target.clone()])?;
        if definition_changed && previous.loaded {
            self.require_launchctl(vec!["bootout".to_string(), target.clone()])?;
        }
        if definition_changed || !previous.loaded {
            let path = definition_path.to_str().ok_or_else(|| {
                format!(
                    "launchd definition path is not valid UTF-8: {}",
                    definition_path.display()
                )
            })?;
            self.require_launchctl(vec![
                "bootstrap".to_string(),
                self.domain(),
                path.to_string(),
            ])?;
        } else if restart_running || previous.run_state != RunState::Running {
            self.require_launchctl(vec!["kickstart".to_string(), "-k".to_string(), target])?;
        }
        Ok(())
    }

    fn down(&self) -> Result<(), String> {
        let status = self.status()?;
        let target = self.target();
        if status.loaded {
            self.require_launchctl(vec!["bootout".to_string(), target.clone()])?;
        }
        self.require_launchctl(vec!["disable".to_string(), target])
    }
}

fn property<'a>(contents: &'a str, name: &str) -> Option<&'a str> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}=")))
        .map(str::trim)
}

fn property_with_spaces<'a>(contents: &'a str, name: &str) -> Option<&'a str> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix(name)
            .and_then(|rest| rest.trim().strip_prefix('='))
            .map(str::trim)
    })
}

fn command_failure(program: &str, args: &[&str], output: &CommandOutput) -> String {
    let detail = if output.stderr.trim().is_empty() {
        output.stdout.trim()
    } else {
        output.stderr.trim()
    };
    format!(
        "{} {} failed with exit code {}: {}",
        program,
        args.join(" "),
        output.code,
        detail
    )
}

fn is_missing(output: &CommandOutput) -> bool {
    let text = format!("{}\n{}", output.stdout, output.stderr).to_ascii_lowercase();
    text.contains("does not exist")
        || text.contains("not found")
        || text.contains("could not find service")
        || text.contains("no such process")
}

fn parse_systemd_home(definition: &str) -> Result<PathBuf, String> {
    let prefix = "Environment=\"PONTIA_HOME=";
    let values = definition
        .lines()
        .filter_map(|line| {
            line.strip_prefix(prefix)
                .and_then(|line| line.strip_suffix('"'))
        })
        .collect::<Vec<_>>();
    if values.len() != 1 {
        return Err(
            "pontia.service must contain exactly one PONTIA_HOME environment value".to_string(),
        );
    }
    Ok(PathBuf::from(systemd_unescape(values[0])?))
}

fn systemd_unescape(value: &str) -> Result<String, String> {
    let mut chars = value.chars().peekable();
    let mut decoded = String::new();
    while let Some(character) = chars.next() {
        match character {
            '\\' => match chars.next() {
                Some('\\') => decoded.push('\\'),
                Some('"') => decoded.push('"'),
                Some('n') => decoded.push('\n'),
                Some('r') => decoded.push('\r'),
                Some('t') => decoded.push('\t'),
                _ => return Err("pontia.service contains an unsupported escape".to_string()),
            },
            '%' if chars.next_if_eq(&'%').is_some() => decoded.push('%'),
            '%' => return Err("pontia.service contains an unescaped systemd specifier".to_string()),
            character => decoded.push(character),
        }
    }
    Ok(decoded)
}

fn parse_launchd_home(definition: &str) -> Result<PathBuf, String> {
    if !definition.starts_with("<?xml ") || !definition.trim_end().ends_with("</plist>") {
        return Err("launchd definition is not a complete XML plist".to_string());
    }
    let marker = "<key>PONTIA_HOME</key>\n    <string>";
    let mut matches = definition.match_indices(marker);
    let (marker_index, _) = matches
        .next()
        .ok_or_else(|| "launchd definition is missing PONTIA_HOME".to_string())?;
    if matches.next().is_some() {
        return Err("launchd definition contains multiple PONTIA_HOME values".to_string());
    }
    let value = &definition[marker_index + marker.len()..];
    let value = value
        .split_once("</string>")
        .map(|(value, _)| value)
        .ok_or_else(|| "launchd PONTIA_HOME value is malformed".to_string())?;
    Ok(PathBuf::from(xml_unescape(value)?))
}

fn xml_unescape(value: &str) -> Result<String, String> {
    if value.contains('<') {
        return Err("launchd PONTIA_HOME value contains unexpected XML markup".to_string());
    }
    let mut decoded = String::new();
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        decoded.push_str(&rest[..index]);
        rest = &rest[index..];
        let (entity, character) = [
            ("&amp;", '&'),
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
            ("&apos;", '\''),
        ]
        .into_iter()
        .find(|(entity, _)| rest.starts_with(entity))
        .ok_or_else(|| "launchd PONTIA_HOME value contains an unknown XML entity".to_string())?;
        decoded.push(character);
        rest = &rest[entity.len()..];
    }
    decoded.push_str(rest);
    Ok(decoded)
}
