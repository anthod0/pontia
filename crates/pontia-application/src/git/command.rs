//! Read-only git invocation with a timeout and an output cap.

use pontia_core::error::{Error, Result};
use std::{collections::BTreeSet, path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

const GIT_TIMEOUT: Duration = Duration::from_secs(5);
const STDERR_LIMIT: usize = 64 * 1024;

pub(crate) struct GitOutput {
    pub stdout: Vec<u8>,
    /// The output was cut at the requested limit and the process stopped.
    pub truncated: bool,
}

/// Runs git in `dir` without a shell. Arguments are passed as separate argv entries and
/// pathspecs are literal. Repository settings that run external programs or take locks are
/// disabled. Exit code 1 is accepted when `allow_exit_one` is set (`git diff --no-index`).
pub(crate) async fn run_git(
    dir: &Path,
    args: &[&str],
    limit: usize,
    allow_exit_one: bool,
) -> Result<GitOutput> {
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
    let overrides = filter_overrides(&config.stdout)?;
    invoke(dir, &overrides, args, limit, allow_exit_one).await
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
    let label = args.join(" ");
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
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    // Stderr is drained to the end so git never blocks on it; only its start is kept.
    let stderr_task = tokio::spawn(async move {
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
    });

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
    let result = tokio::time::timeout(GIT_TIMEOUT, run).await;
    let (stdout, status) = match result {
        Ok(result) => result?,
        Err(_) => {
            stderr_task.abort();
            return Err(Error::Domain(format!("git {label} timed out")));
        }
    };
    let stderr = stderr_task.await.unwrap_or_default();

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

/// Runs a git query whose whole output is needed.
pub(crate) async fn run_git_text(dir: &Path, args: &[&str]) -> Result<String> {
    const LIMIT: usize = 8 * 1024 * 1024;
    let output = run_git(dir, args, LIMIT, false).await?;
    if output.truncated {
        return Err(Error::Domain(format!(
            "git {} output exceeds {LIMIT} bytes",
            args.join(" ")
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Resolves the top level of the work tree containing `dir`, or `None` outside a repository.
pub(crate) async fn repo_root(dir: &Path) -> Result<Option<String>> {
    match run_git_text(dir, &["rev-parse", "--show-toplevel"]).await {
        Ok(root) => Ok(Some(root.trim_end_matches('\n').to_string())),
        Err(Error::Domain(message)) if message.contains("not a git repository") => Ok(None),
        Err(error) => Err(error),
    }
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
        run_git_text(repo.path(), &["init", "-q"]).await.unwrap();
        std::fs::write(repo.path().join("big.txt"), "line\n".repeat(400_000)).unwrap();

        let started = std::time::Instant::now();
        let output = run_git(
            repo.path(),
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
