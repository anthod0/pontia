//! Bounded git invocation with a timeout and an output cap.

use pontia_core::error::{Error, Result};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

const GIT_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_TIMEOUT: Duration = Duration::from_secs(60);
const STDERR_LIMIT: usize = 64 * 1024;

pub(crate) struct GitOutput {
    pub stdout: Vec<u8>,
    /// The output was cut at the requested limit and the process stopped.
    pub truncated: bool,
}

pub(crate) struct GitCommand {
    dir: PathBuf,
    overrides: Vec<String>,
}

impl GitCommand {
    pub(crate) async fn new(dir: &Path) -> Result<Self> {
        // Read names only. Included files inherit the scope of the including config.
        let config = invoke(
            dir,
            &[],
            &[
                "config",
                "--null",
                "--show-scope",
                "--name-only",
                "--get-regexp",
                r"^filter\..*\.(clean|smudge|process)$",
            ],
            1024 * 1024,
            true,
        )
        .await?;
        if config.truncated {
            return Err(Error::Domain(
                "git filter configuration exceeds limit".into(),
            ));
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            overrides: filter_overrides(&config.stdout)?,
        })
    }

    pub(crate) async fn run(
        &self,
        args: &[&str],
        limit: usize,
        allow_exit_one: bool,
    ) -> Result<GitOutput> {
        invoke(&self.dir, &self.overrides, args, limit, allow_exit_one).await
    }

    pub(crate) async fn run_text(&self, args: &[&str]) -> Result<String> {
        const LIMIT: usize = 8 * 1024 * 1024;
        let output = self.run(args, LIMIT, false).await?;
        if output.truncated {
            return Err(Error::Domain(format!(
                "git {} output exceeds {LIMIT} bytes",
                args.join(" ")
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    // Writes use Git's configured clean/smudge filters so the index and checkout
    // contain the same content as they would after an ordinary Git operation.
    pub(crate) fn enable_write_filters(&mut self) {
        self.overrides.clear();
    }

    pub(crate) async fn check_discard_filters(&self, paths: &[&str]) -> Result<()> {
        if self.overrides.is_empty() {
            return Ok(());
        }
        let mut args = vec!["check-attr", "-z", "filter", "--"];
        args.extend(paths);
        let attrs = self.run_text(&args).await?;
        let mut fields = attrs.split_terminator('\0');
        while let Some(path) = fields.next() {
            let driver = fields
                .nth(1)
                .ok_or_else(|| Error::Domain("Invalid git check-attr output".into()))?;
            if self.overrides.contains(&format!("filter.{driver}.clean=")) {
                return Err(Error::Domain(format!(
                    "Cannot safely restore {path:?} with repository-defined Git filter {driver:?}"
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }
}

fn filter_overrides(config: &[u8]) -> Result<Vec<String>> {
    let config = std::str::from_utf8(config)
        .map_err(|_| Error::Domain("invalid git filter configuration".into()))?;
    let mut fields = config.split_terminator('\0');
    let mut drivers = BTreeSet::new();
    while let Some(scope) = fields.next() {
        let key = fields
            .next()
            .ok_or_else(|| Error::Domain("missing git config key".into()))?;
        if matches!(scope, "local" | "worktree") {
            let (driver, _) = key
                .rsplit_once('.')
                .ok_or_else(|| Error::Domain("invalid git filter key".into()))?;
            if driver.contains('=') {
                return Err(Error::Domain("git filter name cannot be overridden".into()));
            }
            drivers.insert(driver);
        }
    }
    Ok(drivers
        .into_iter()
        .flat_map(|driver| {
            ["clean=", "smudge=", "process=", "required=false"]
                .map(|setting| format!("{driver}.{setting}"))
        })
        .collect())
}

async fn invoke(
    dir: &Path,
    overrides: &[String],
    args: &[&str],
    limit: usize,
    allow_exit_one: bool,
) -> Result<GitOutput> {
    let label = args.first().copied().unwrap_or("command");
    let remote = matches!(label, "pull" | "push");
    let timeout = if remote { REMOTE_TIMEOUT } else { GIT_TIMEOUT };
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.quotePath=false",
            // Signature checks print before the requested format and run `gpg.program`.
            "-c",
            "log.showSignature=false",
        ])
        .args(
            overrides
                .iter()
                .flat_map(|setting| ["-c", setting.as_str()]),
        )
        .args(args)
        .env_remove("GIT_CONFIG")
        .env("GIT_DIR", dir.join(".git"))
        .env("GIT_WORK_TREE", dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("GIT_ASKPASS", "false")
        .env("SSH_ASKPASS", "false")
        .env("GIT_EDITOR", "false")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()?;
    // A transport or credential helper can outlive git and retain its pipes.
    let mut group = GitProcessGroup(
        child
            .id()
            .and_then(|id| rustix::process::Pid::from_raw(id as i32)),
    );
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    // Stderr is drained to the end so git never blocks on it; only its start is kept.
    let read_stderr = async move {
        let mut kept = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            match stderr.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let room = STDERR_LIMIT.saturating_sub(kept.len());
                    kept.extend_from_slice(&buffer[..count.min(room)]);
                }
            }
        }
        kept
    };

    let run = async {
        let mut out = Vec::new();
        (&mut stdout)
            .take(limit as u64 + 1)
            .read_to_end(&mut out)
            .await?;
        if out.len() > limit {
            // Stop git as soon as the cap is reached instead of waiting for it to finish.
            out.truncate(limit);
            child.start_kill()?;
            child.wait().await?;
            return Ok::<_, std::io::Error>((out, None));
        }
        let status = child.wait().await?;
        Ok((out, Some(status)))
    };
    let result = tokio::time::timeout(timeout, async { tokio::join!(run, read_stderr) }).await;
    let (output, stderr) = result.map_err(|_| Error::Domain(format!("git {label} timed out")))?;
    let (stdout, status) = output?;
    if status.is_some() {
        group.0 = None;
    }

    let Some(status) = status else {
        return Ok(GitOutput {
            stdout,
            truncated: true,
        });
    };
    if !status.success() && !(allow_exit_one && status.code() == Some(1)) {
        let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
        return Err(Error::Domain(if stderr.is_empty() {
            format!("git {label} failed")
        } else {
            stderr
        }));
    }
    Ok(GitOutput {
        stdout,
        truncated: false,
    })
}

struct GitProcessGroup(Option<rustix::process::Pid>);

impl Drop for GitProcessGroup {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
    }
}

/// Git queries are scoped to the Workspace's own .git directory or gitfile.
pub(crate) async fn repo_root(command: &GitCommand) -> Result<Option<String>> {
    match tokio::fs::symlink_metadata(command.dir.join(".git")).await {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let root = command.run_text(&["rev-parse", "--show-toplevel"]).await?;
    Ok(Some(root.trim_end_matches('\n').to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disables_only_drivers_with_repository_commands() {
        let config = b"global\0filter.lfs.process\0local\0filter.Custom.Driver.clean\0worktree\0filter.Custom.Driver.smudge\0";
        assert_eq!(
            filter_overrides(config).unwrap(),
            [
                "filter.Custom.Driver.clean=",
                "filter.Custom.Driver.smudge=",
                "filter.Custom.Driver.process=",
                "filter.Custom.Driver.required=false"
            ]
        );
    }

    #[tokio::test]
    async fn stops_git_when_output_reaches_the_limit() {
        let repo = tempfile::tempdir().unwrap();
        let command = GitCommand::new(repo.path()).await.unwrap();
        command.run_text(&["init", "-q"]).await.unwrap();
        std::fs::write(repo.path().join("big.txt"), "line\n".repeat(400_000)).unwrap();

        let started = std::time::Instant::now();
        let output = command
            .run(
                &["diff", "--no-index", "--", "/dev/null", "big.txt"],
                64 * 1024,
                true,
            )
            .await
            .unwrap();

        assert!(output.truncated);
        assert_eq!(output.stdout.len(), 64 * 1024);
        assert!(started.elapsed() < GIT_TIMEOUT);
    }
}
