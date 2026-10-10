use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

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

pub fn launchd_executable_search_path(
    inherited_path: Option<&OsStr>,
    user_home: &Path,
    preferred_paths: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    let current_dir = env::current_dir()
        .map_err(|error| format!("failed to resolve the current directory: {error}"))?;
    let mut paths = Vec::new();

    let mut push_unique = |path: PathBuf| {
        let path = if path.is_absolute() {
            path
        } else {
            current_dir.join(path)
        };
        if !paths.contains(&path) {
            paths.push(path);
        }
    };

    for path in preferred_paths {
        push_unique(path.clone());
    }
    if let Some(path) = inherited_path {
        for directory in env::split_paths(path) {
            push_unique(directory);
        }
    }
    for directory in [
        user_home.join(".local/bin"),
        user_home.join(".bun/bin"),
        user_home.join(".cargo/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ] {
        push_unique(directory);
    }

    Ok(paths)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn launchd_search_path_prioritizes_normalizes_and_deduplicates_directories() {
        let current_dir = env::current_dir().expect("current directory");
        let paths = launchd_executable_search_path(
            Some(OsStr::new("tools:/opt/homebrew/bin:/preferred")),
            Path::new("/Users/alice"),
            &[PathBuf::from("/preferred")],
        )
        .expect("launchd search path");

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/preferred"),
                current_dir.join("tools"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/Users/alice/.local/bin"),
                PathBuf::from("/Users/alice/.bun/bin"),
                PathBuf::from("/Users/alice/.cargo/bin"),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
                PathBuf::from("/usr/sbin"),
                PathBuf::from("/sbin"),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn launchd_search_path_supplies_defaults_without_an_inherited_path() {
        let paths = launchd_executable_search_path(None, Path::new("/Users/alice"), &[])
            .expect("launchd search path");

        assert_eq!(
            paths.first(),
            Some(&PathBuf::from("/Users/alice/.local/bin"))
        );
        assert!(paths.contains(&PathBuf::from("/usr/bin")));
    }
}
