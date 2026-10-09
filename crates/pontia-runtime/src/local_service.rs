use std::{path::Path, process::Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait CommandRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String>;

    fn run_with_env(
        &self,
        program: &str,
        args: &[String],
        environment: &[(String, String)],
    ) -> Result<CommandOutput, String> {
        if environment.is_empty() {
            self.run(program, args)
        } else {
            Err(format!(
                "command runner does not support an explicit environment for {program}"
            ))
        }
    }

    /// Captures output without interpreting it and returns only the exit code.
    fn run_status(&self, program: &str, args: &[String]) -> Result<i32, String> {
        self.run(program, args).map(|output| output.code)
    }

    fn run_interactive(&self, program: &str, args: &[String]) -> Result<i32, String> {
        self.run(program, args).map(|output| output.code)
    }
}

#[derive(Debug, Default)]
pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        run_process(program, args, &[])
    }

    fn run_with_env(
        &self,
        program: &str,
        args: &[String],
        environment: &[(String, String)],
    ) -> Result<CommandOutput, String> {
        run_process(program, args, environment)
    }

    fn run_status(&self, program: &str, args: &[String]) -> Result<i32, String> {
        Command::new(program)
            .args(args)
            .output()
            .map(|output| output.status.code().unwrap_or(-1))
            .map_err(|error| format!("failed to execute {program}: {error}"))
    }

    fn run_interactive(&self, program: &str, args: &[String]) -> Result<i32, String> {
        Command::new(program)
            .args(args)
            .status()
            .map(|status| status.code().unwrap_or(-1))
            .map_err(|error| format!("failed to execute {program}: {error}"))
    }
}

fn run_process(
    program: &str,
    args: &[String],
    environment: &[(String, String)],
) -> Result<CommandOutput, String> {
    let output = Command::new(program)
        .args(args)
        .envs(environment.iter().map(|(key, value)| (key, value)))
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

pub trait DefinitionStore {
    fn read(&self, path: &Path) -> Result<Option<String>, String>;
    fn install(&self, path: &Path, contents: &str) -> Result<bool, String>;
}

pub fn absolute_utf8_path<'a>(path: &'a Path, description: &str) -> Result<&'a str, String> {
    if !path.is_absolute() {
        return Err(format!(
            "{description} must be an absolute path: {}",
            path.display()
        ));
    }
    path.to_str()
        .ok_or_else(|| format!("{description} is not valid UTF-8: {}", path.display()))
}

pub fn xml_escape(value: &str) -> Result<String, String> {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() && !matches!(character, '\t' | '\n' | '\r') {
            return Err("value contains a character that XML 1.0 cannot represent".into());
        }
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            character => escaped.push(character),
        }
    }
    Ok(escaped)
}

pub fn systemd_quote(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '%' => escaped.push_str("%%"),
            character => escaped.push(character),
        }
    }
    escaped
}
