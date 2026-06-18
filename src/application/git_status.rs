use super::*;
use pontia_storage_sqlite::repositories::git_status::{
    GitStatusUpsertRecord, SqliteGitStatusRepository,
};
use std::{
    collections::HashMap,
    future::Future,
    process::Command,
    sync::Arc,
    sync::mpsc,
    time::{Duration, Instant},
};

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

const GIT_REFRESH_TTL: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct GitRefreshCoordinator {
    ttl: Duration,
    entries: Arc<tokio::sync::Mutex<HashMap<String, Arc<GitRefreshEntry>>>>,
}

struct GitRefreshEntry {
    singleflight: tokio::sync::Mutex<()>,
    cache: tokio::sync::Mutex<Option<CachedGitRefresh>>,
}

struct CachedGitRefresh {
    refreshed_at: Instant,
    view: WorkspaceGitStatusView,
}

impl GitRefreshCoordinator {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub async fn refresh<F, Fut>(
        &self,
        workspace_id: &str,
        refresh: F,
    ) -> Result<WorkspaceGitStatusView>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<WorkspaceGitStatusView>>,
    {
        let entry = self.entry(workspace_id).await;
        let _singleflight = entry.singleflight.lock().await;

        if let Some(view) = entry.fresh_cached_view(self.ttl).await {
            return Ok(view);
        }

        let view = refresh().await?;
        entry.store(view.clone()).await;
        Ok(view)
    }

    async fn entry(&self, workspace_id: &str) -> Arc<GitRefreshEntry> {
        let mut entries = self.entries.lock().await;
        entries
            .entry(workspace_id.to_string())
            .or_insert_with(|| Arc::new(GitRefreshEntry::new()))
            .clone()
    }
}

impl Default for GitRefreshCoordinator {
    fn default() -> Self {
        Self::new(GIT_REFRESH_TTL)
    }
}

impl GitRefreshEntry {
    fn new() -> Self {
        Self {
            singleflight: tokio::sync::Mutex::new(()),
            cache: tokio::sync::Mutex::new(None),
        }
    }

    async fn fresh_cached_view(&self, ttl: Duration) -> Option<WorkspaceGitStatusView> {
        self.cache
            .lock()
            .await
            .as_ref()
            .filter(|cached| cached.refreshed_at.elapsed() < ttl)
            .map(|cached| cached.view.clone())
    }

    async fn store(&self, view: WorkspaceGitStatusView) {
        *self.cache.lock().await = Some(CachedGitRefresh {
            refreshed_at: Instant::now(),
            view,
        });
    }
}

pub struct WorkspaceGitStatusService {
    pool: SqlitePool,
    refresh_coordinator: GitRefreshCoordinator,
}

impl WorkspaceGitStatusService {
    pub fn new(pool: SqlitePool, refresh_coordinator: GitRefreshCoordinator) -> Self {
        Self {
            pool,
            refresh_coordinator,
        }
    }

    pub async fn refresh_workspace_git_status(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceGitStatusView> {
        let pool = self.pool.clone();
        self.refresh_coordinator
            .refresh(workspace_id, || async move {
                refresh_workspace_git_status_now(pool, workspace_id).await
            })
            .await
    }
}

async fn refresh_workspace_git_status_now(
    pool: SqlitePool,
    workspace_id: &str,
) -> Result<WorkspaceGitStatusView> {
    let workspace = get_workspace_record(&pool, workspace_id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("workspace {workspace_id} not found")))?;

    let repository = SqliteGitStatusRepository::new(pool.clone());
    let observed_at = repository.current_timestamp().await?;
    let outcome = observe_git_status(&workspace.canonical_path).await;
    match outcome {
        Ok(parsed) => {
            let clean = parsed.staged_count == 0
                && parsed.unstaged_count == 0
                && parsed.untracked_count == 0
                && parsed.conflicted_count == 0;
            upsert_git_status(
                &repository,
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
                &repository,
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

    ExternalQueryService::new(pool.clone())
        .get_workspace_git_status(workspace_id)
        .await?
        .ok_or_else(|| {
            Error::NotFound(format!("git status for workspace {workspace_id} not found"))
        })
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
    repository: &SqliteGitStatusRepository,
    workspace_id: &str,
    parsed: &ParsedGitStatus,
    clean: bool,
    state: &str,
    failure: Option<String>,
    observed_at: &str,
) -> Result<()> {
    repository
        .upsert_status(GitStatusUpsertRecord {
            workspace_id: workspace_id.to_string(),
            repo_root: parsed.repo_root.clone(),
            branch: parsed.branch.clone(),
            upstream: parsed.upstream.clone(),
            ahead: parsed.ahead,
            behind: parsed.behind,
            staged_count: parsed.staged_count,
            unstaged_count: parsed.unstaged_count,
            untracked_count: parsed.untracked_count,
            conflicted_count: parsed.conflicted_count,
            clean,
            state: state.to_string(),
            failure,
            observed_at: observed_at.to_string(),
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status_view(workspace_id: &str, branch: &str) -> WorkspaceGitStatusView {
        WorkspaceGitStatusView {
            workspace_id: workspace_id.to_string(),
            repo_root: Some(format!("/tmp/{workspace_id}")),
            branch: Some(branch.to_string()),
            upstream: None,
            ahead: 0,
            behind: 0,
            staged_count: 0,
            unstaged_count: 0,
            untracked_count: 0,
            conflicted_count: 0,
            clean: true,
            state: "observed".to_string(),
            failure: None,
            observed_at: Some(branch.to_string()),
            updated_at: Some(branch.to_string()),
        }
    }

    #[tokio::test]
    async fn git_refresh_coordinator_reuses_fresh_result_within_ttl() {
        let coordinator = GitRefreshCoordinator::new(Duration::from_secs(60));
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let first = coordinator
            .refresh("workspace-1", {
                let calls = calls.clone();
                move || {
                    let calls = calls.clone();
                    async move {
                        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok(status_view("workspace-1", "first"))
                    }
                }
            })
            .await
            .expect("first refresh");
        let second = coordinator
            .refresh("workspace-1", {
                let calls = calls.clone();
                move || {
                    let calls = calls.clone();
                    async move {
                        calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        Ok(status_view("workspace-1", "second"))
                    }
                }
            })
            .await
            .expect("second refresh");

        assert_eq!(first.branch.as_deref(), Some("first"));
        assert_eq!(second.branch.as_deref(), Some("first"));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn git_refresh_coordinator_singleflights_concurrent_refreshes() {
        let coordinator = std::sync::Arc::new(GitRefreshCoordinator::new(Duration::from_secs(60)));
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));

        let mut tasks = Vec::new();
        for _ in 0..2 {
            let coordinator = coordinator.clone();
            let calls = calls.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                coordinator
                    .refresh("workspace-1", move || {
                        let calls = calls.clone();
                        async move {
                            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            Ok(status_view("workspace-1", "main"))
                        }
                    })
                    .await
                    .expect("refresh")
            }));
        }

        let first = tasks.pop().unwrap().await.expect("task");
        let second = tasks.pop().unwrap().await.expect("task");

        assert_eq!(first.branch.as_deref(), Some("main"));
        assert_eq!(second.branch.as_deref(), Some("main"));
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

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
