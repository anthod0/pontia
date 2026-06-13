use super::*;
use std::{process::Command, sync::mpsc, time::Duration};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ParsedGitStatus {
    repo_root: Option<String>,
    branch: Option<String>,
    upstream: Option<String>,
    ahead: i64,
    behind: i64,
    staged_count: i64,
    unstaged_count: i64,
    untracked_count: i64,
    conflicted_count: i64,
}

pub struct WorkspaceGitStatusService {
    pool: SqlitePool,
}

impl WorkspaceGitStatusService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn refresh_workspace_git_status(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceGitStatusView> {
        let workspace = get_workspace_record(&self.pool, workspace_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("workspace {workspace_id} not found")))?;

        let observed_at = current_timestamp(&self.pool).await?;
        let outcome = observe_git_status(&workspace.canonical_path).await;
        match outcome {
            Ok(parsed) => {
                let clean = parsed.staged_count == 0
                    && parsed.unstaged_count == 0
                    && parsed.untracked_count == 0
                    && parsed.conflicted_count == 0;
                upsert_git_status(
                    &self.pool,
                    workspace_id,
                    &parsed,
                    clean,
                    "observed",
                    None,
                    &observed_at,
                )
                .await?;
            }
            Err(error) => {
                let parsed = ParsedGitStatus::default();
                upsert_git_status(
                    &self.pool,
                    workspace_id,
                    &parsed,
                    true,
                    "error",
                    Some(error.to_string()),
                    &observed_at,
                )
                .await?;
            }
        }

        ExternalQueryService::new(self.pool.clone())
            .get_workspace_git_status(workspace_id)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!("git status for workspace {workspace_id} not found"))
            })
    }
}

async fn observe_git_status(workspace_path: &str) -> Result<ParsedGitStatus> {
    let repo_root = run_git(workspace_path, &["rev-parse", "--show-toplevel"]).await?;
    let status = run_git(workspace_path, &["status", "--porcelain=v2", "--branch"]).await?;
    let mut parsed = parse_porcelain_v2_status(&status)?;
    parsed.repo_root = Some(repo_root.trim().to_string());
    Ok(parsed)
}

async fn run_git(workspace_path: &str, args: &[&str]) -> Result<String> {
    let workspace_path = workspace_path.to_string();
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    tokio::task::spawn_blocking(move || run_git_blocking(workspace_path, args))
        .await
        .map_err(|err| Error::Domain(format!("git task join failed: {err}")))?
}

fn run_git_blocking(workspace_path: String, args: Vec<String>) -> Result<String> {
    let command_label = args.join(" ");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let output = Command::new("git")
            .args(["-C", workspace_path.as_str()])
            .args(&args)
            .output();
        let _ = sender.send(output);
    });

    let output = receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| Error::Domain(format!("git {command_label} timed out")))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(Error::Domain(if stderr.is_empty() {
            format!("git {command_label} failed")
        } else {
            stderr
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_porcelain_v2_status(output: &str) -> Result<ParsedGitStatus> {
    let mut parsed = ParsedGitStatus::default();
    for line in output.lines() {
        if let Some(branch) = line.strip_prefix("# branch.head ") {
            if branch != "(detached)" {
                parsed.branch = Some(branch.to_string());
            }
            continue;
        }
        if let Some(upstream) = line.strip_prefix("# branch.upstream ") {
            parsed.upstream = Some(upstream.to_string());
            continue;
        }
        if let Some(ab) = line.strip_prefix("# branch.ab ") {
            for part in ab.split_whitespace() {
                if let Some(value) = part.strip_prefix('+') {
                    parsed.ahead = value
                        .parse()
                        .map_err(|_| Error::Domain(format!("invalid git ahead count: {part}")))?;
                } else if let Some(value) = part.strip_prefix('-') {
                    parsed.behind = value
                        .parse()
                        .map_err(|_| Error::Domain(format!("invalid git behind count: {part}")))?;
                }
            }
            continue;
        }

        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("?") => parsed.untracked_count += 1,
            Some("u") => parsed.conflicted_count += 1,
            Some("1" | "2") => {
                if let Some(xy) = parts.next() {
                    let mut chars = xy.chars();
                    let x = chars.next().unwrap_or('.');
                    let y = chars.next().unwrap_or('.');
                    if x != '.' {
                        parsed.staged_count += 1;
                    }
                    if y != '.' {
                        parsed.unstaged_count += 1;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(parsed)
}

async fn upsert_git_status(
    pool: &SqlitePool,
    workspace_id: &str,
    parsed: &ParsedGitStatus,
    clean: bool,
    state: &str,
    failure: Option<String>,
    observed_at: &str,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO workspace_git_status
           (workspace_id, repo_root, branch, upstream, ahead, behind, staged_count, unstaged_count,
            untracked_count, conflicted_count, clean, state, failure, observed_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
           ON CONFLICT(workspace_id) DO UPDATE SET
             repo_root = excluded.repo_root,
             branch = excluded.branch,
             upstream = excluded.upstream,
             ahead = excluded.ahead,
             behind = excluded.behind,
             staged_count = excluded.staged_count,
             unstaged_count = excluded.unstaged_count,
             untracked_count = excluded.untracked_count,
             conflicted_count = excluded.conflicted_count,
             clean = excluded.clean,
             state = excluded.state,
             failure = excluded.failure,
             observed_at = excluded.observed_at,
             updated_at = excluded.updated_at"#,
    )
    .bind(workspace_id)
    .bind(&parsed.repo_root)
    .bind(&parsed.branch)
    .bind(&parsed.upstream)
    .bind(parsed.ahead)
    .bind(parsed.behind)
    .bind(parsed.staged_count)
    .bind(parsed.unstaged_count)
    .bind(parsed.untracked_count)
    .bind(parsed.conflicted_count)
    .bind(clean)
    .bind(state)
    .bind(failure)
    .bind(observed_at)
    .execute(pool)
    .await?;
    Ok(())
}

async fn current_timestamp(pool: &SqlitePool) -> Result<String> {
    Ok(
        sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')")
            .fetch_one(pool)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2_summary_counts() {
        let parsed = parse_porcelain_v2_status(
            "# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 a b README.md\n1 A. N... 000000 100644 100644 a b src/lib.rs\n? notes.txt\nu UU N... 100644 100644 100644 100644 a b c d file.txt\n",
        )
        .expect("parse");

        assert_eq!(parsed.branch.as_deref(), Some("main"));
        assert_eq!(parsed.upstream.as_deref(), Some("origin/main"));
        assert_eq!(parsed.ahead, 2);
        assert_eq!(parsed.behind, 1);
        assert_eq!(parsed.staged_count, 1);
        assert_eq!(parsed.unstaged_count, 1);
        assert_eq!(parsed.untracked_count, 1);
        assert_eq!(parsed.conflicted_count, 1);
    }
}
