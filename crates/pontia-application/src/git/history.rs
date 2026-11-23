//! Branches, commit history and commit contents. Everything here reads committed objects
//! only; nothing checks out a branch or touches the work tree.

use pontia_core::error::{Error, Result};
use serde::Serialize;
use std::path::Path;

use super::{
    changes::{MAX_CHANGED_FILES, Numstat, parse_numstat},
    command::{run_git, run_git_text},
    diff::{DiffHunkView, MAX_DIFF_BYTES, parse_unified_diff, validate_repo_path},
};

/// Commits returned per page of history.
pub const COMMIT_PAGE_SIZE: usize = 50;
/// Field separator in `--format` output; it cannot occur in names or subjects git prints.
const FIELD: char = '\x1f';
/// The ref shown for a detached HEAD.
const DETACHED_HEAD: &str = "HEAD";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitBranchView {
    /// Short name, such as `main` or `origin/main`.
    pub name: String,
    /// Full ref name, such as `refs/heads/main`.
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub remote: bool,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitBranchesView {
    /// `None` when the Workspace is not inside a Git work tree.
    pub repo_root: Option<String>,
    /// The checked-out branch ref, `HEAD` when detached, or `None` before the first commit.
    pub head: Option<String>,
    pub branches: Vec<GitBranchView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitCommitSummaryView {
    pub sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub authored_at: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitCommitsView {
    pub commits: Vec<GitCommitSummaryView>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitCommitFileView {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<String>,
    /// One of `M`, `A`, `D`, `R`, `C` or `T`.
    pub status: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub binary: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitCommitView {
    pub sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    pub committer_name: String,
    pub committed_at: String,
    pub subject: String,
    /// The message after the subject line, without surrounding blank lines.
    pub body: String,
    /// Files changed against the first parent; a root commit lists every file it adds.
    pub files: Vec<GitCommitFileView>,
    pub total: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitCommitDiffView {
    pub sha: String,
    pub path: String,
    pub binary: bool,
    pub hunks: Vec<DiffHunkView>,
    pub truncated: bool,
}

pub(crate) async fn branches(repo_root: &Path) -> Result<(Option<String>, Vec<GitBranchView>)> {
    let head = head_ref(repo_root).await?;
    let output = run_git_text(
        repo_root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname)%1f%(symref)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .await?;
    let branches = output
        .lines()
        .filter_map(|line| {
            let (ref_name, symref) = line.split_once(FIELD)?;
            // `origin/HEAD` points at another remote branch and is not a branch itself.
            if !symref.is_empty() {
                return None;
            }
            let (name, remote) = if let Some(name) = ref_name.strip_prefix("refs/heads/") {
                (name, false)
            } else {
                (ref_name.strip_prefix("refs/remotes/")?, true)
            };
            Some(GitBranchView {
                name: name.to_string(),
                ref_name: ref_name.to_string(),
                remote,
                current: head.as_deref() == Some(ref_name),
            })
        })
        .collect();
    Ok((head, branches))
}

/// The checked-out branch, `HEAD` when detached, or `None` while HEAD has no commit.
async fn head_ref(repo_root: &Path) -> Result<Option<String>> {
    let commit = run_git(
        repo_root,
        &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
        4096,
        true,
    )
    .await?;
    if commit.stdout.is_empty() {
        return Ok(None);
    }
    let symbolic = run_git(repo_root, &["symbolic-ref", "--quiet", "HEAD"], 4096, true).await?;
    let name = String::from_utf8_lossy(&symbolic.stdout).trim().to_string();
    Ok(Some(if name.is_empty() {
        DETACHED_HEAD.to_string()
    } else {
        name
    }))
}

pub(crate) async fn commits(
    repo_root: &Path,
    ref_name: &str,
    skip: usize,
) -> Result<GitCommitsView> {
    validate_ref(ref_name)?;
    let count = format!("--max-count={}", COMMIT_PAGE_SIZE + 1);
    let skip = format!("--skip={skip}");
    let output = run_git_text(
        repo_root,
        &[
            "log",
            "-z",
            &count,
            &skip,
            "--format=%H%x1f%P%x1f%an%x1f%aI%x1f%s",
            "--end-of-options",
            ref_name,
            "--",
        ],
    )
    .await
    .map_err(|error| match error {
        Error::Domain(message) if message.contains("bad revision") => {
            Error::NotFound(format!("ref {ref_name} not found"))
        }
        error => error,
    })?;
    let mut commits = output
        .split('\0')
        .filter(|record| !record.is_empty())
        .map(|record| {
            let fields = record.splitn(5, FIELD).collect::<Vec<_>>();
            let [sha, parents, author_name, authored_at, subject] = fields[..] else {
                return Err(Error::Domain(format!(
                    "unexpected git log record: {record}"
                )));
            };
            Ok(GitCommitSummaryView {
                sha: sha.to_string(),
                parents: split_parents(parents),
                author_name: author_name.to_string(),
                authored_at: authored_at.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let has_more = commits.len() > COMMIT_PAGE_SIZE;
    commits.truncate(COMMIT_PAGE_SIZE);
    Ok(GitCommitsView { commits, has_more })
}

pub(crate) async fn commit(repo_root: &Path, sha: &str) -> Result<GitCommitView> {
    validate_sha(sha)?;
    let object = format!("{sha}^{{commit}}");
    let header = run_git_text(
        repo_root,
        &[
            "show",
            "--no-patch",
            "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%cn%x1f%cI%x1f%B",
            "--end-of-options",
            &object,
        ],
    )
    .await
    .map_err(|error| commit_not_found(error, sha))?;
    let fields = header.splitn(8, FIELD).collect::<Vec<_>>();
    let [
        sha,
        parents,
        author_name,
        author_email,
        authored_at,
        committer_name,
        committed_at,
        message,
    ] = fields[..]
    else {
        return Err(Error::Domain(format!(
            "unexpected git show output for {sha}"
        )));
    };
    let message = message.trim();
    let (subject, body) = message.split_once('\n').unwrap_or((message, ""));

    let tree_args = |format: &'static str| {
        [
            "diff-tree",
            "-r",
            "-z",
            "-M",
            "--root",
            "--diff-merges=first-parent",
            "--no-commit-id",
            "--no-ext-diff",
            "--no-textconv",
            format,
            "--end-of-options",
            sha,
        ]
    };
    let names = run_git_text(repo_root, &tree_args("--name-status")).await?;
    let numstat = run_git_text(repo_root, &tree_args("--numstat")).await?;
    let mut files = parse_name_status(&names)?;
    let total = files.len();
    files.truncate(MAX_CHANGED_FILES);
    let stats = parse_numstat(&numstat);
    for file in &mut files {
        match stats.get(&file.path) {
            Some(Numstat::Text(additions, deletions)) => {
                file.additions = Some(*additions);
                file.deletions = Some(*deletions);
            }
            Some(Numstat::Binary) => file.binary = true,
            None => {}
        }
    }
    Ok(GitCommitView {
        sha: sha.to_string(),
        parents: split_parents(parents),
        author_name: author_name.to_string(),
        author_email: author_email.to_string(),
        authored_at: authored_at.to_string(),
        committer_name: committer_name.to_string(),
        committed_at: committed_at.to_string(),
        subject: subject.trim().to_string(),
        body: body.trim().to_string(),
        truncated: total > files.len(),
        files,
        total,
    })
}

pub(crate) async fn commit_diff(
    repo_root: &Path,
    sha: &str,
    path: &str,
    orig_path: Option<&str>,
) -> Result<GitCommitDiffView> {
    validate_sha(sha)?;
    validate_repo_path(path)?;
    if let Some(orig_path) = orig_path {
        validate_repo_path(orig_path)?;
    }
    let object = format!("{sha}^{{commit}}");
    let mut args = vec![
        "diff-tree",
        "-p",
        "-M",
        "--root",
        "--diff-merges=first-parent",
        "--no-commit-id",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "--end-of-options",
        &object,
        "--",
    ];
    args.extend(orig_path);
    args.push(path);
    let output = run_git(repo_root, &args, MAX_DIFF_BYTES, false)
        .await
        .map_err(|error| commit_not_found(error, sha))?;
    let mut diff = parse_unified_diff(&String::from_utf8_lossy(&output.stdout));
    diff.truncated |= output.truncated;
    Ok(GitCommitDiffView {
        sha: sha.to_string(),
        path: path.to_string(),
        binary: diff.binary,
        hunks: diff.hunks,
        truncated: diff.truncated,
    })
}

fn commit_not_found(error: Error, sha: &str) -> Error {
    match error {
        Error::Domain(message)
            if message.contains("bad revision")
                || message.contains("bad object")
                || message.contains("unknown revision")
                || message.contains("ambiguous argument") =>
        {
            Error::NotFound(format!("commit {sha} not found"))
        }
        error => error,
    }
}

fn split_parents(parents: &str) -> Vec<String> {
    parents.split_whitespace().map(str::to_string).collect()
}

/// Accepts the detached HEAD or a full branch or remote-tracking ref name.
fn validate_ref(ref_name: &str) -> Result<()> {
    let branch = ref_name
        .strip_prefix("refs/heads/")
        .or_else(|| ref_name.strip_prefix("refs/remotes/"));
    let valid = ref_name == DETACHED_HEAD
        || branch.is_some_and(|name| {
            !name.is_empty()
                && !name.contains("..")
                && !name.contains("@{")
                && !name
                    .chars()
                    .any(|c| c.is_ascii_control() || " ~^:?*[\\".contains(c))
        });
    if valid {
        Ok(())
    } else {
        Err(Error::Domain(format!("invalid ref: {ref_name:?}")))
    }
}

/// Accepts a full SHA-1 or SHA-256 object name.
fn validate_sha(sha: &str) -> Result<()> {
    if matches!(sha.len(), 40 | 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(Error::Domain(format!("invalid commit: {sha:?}")))
    }
}

/// Parses `git diff-tree --name-status -z`. Renames and copies carry a score and two paths.
fn parse_name_status(output: &str) -> Result<Vec<GitCommitFileView>> {
    let mut files = Vec::new();
    let mut fields = output.split('\0').filter(|field| !field.is_empty());
    while let Some(status) = fields.next() {
        let invalid = || Error::Domain(format!("unexpected git diff-tree record: {status}"));
        let code = status.chars().next().ok_or_else(invalid)?;
        let first = fields.next().ok_or_else(invalid)?;
        let (path, orig_path) = if matches!(code, 'R' | 'C') {
            (fields.next().ok_or_else(invalid)?, Some(first))
        } else {
            (first, None)
        };
        files.push(GitCommitFileView {
            path: path.to_string(),
            orig_path: orig_path.map(str::to_string),
            status: code.to_string(),
            additions: None,
            deletions: None,
            binary: false,
        });
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_status_with_renames() {
        let files = parse_name_status("M\0src/lib.rs\0R087\0old name.rs\0new name.rs\0A\0b\0")
            .expect("parse");
        let summary = files
            .iter()
            .map(|file| {
                (
                    file.status.as_str(),
                    file.path.as_str(),
                    file.orig_path.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            vec![
                ("M", "src/lib.rs", None),
                ("R", "new name.rs", Some("old name.rs")),
                ("A", "b", None),
            ]
        );
    }

    #[test]
    fn accepts_only_branch_refs_and_full_object_names() {
        for valid in ["HEAD", "refs/heads/main", "refs/remotes/origin/feature/x"] {
            validate_ref(valid).expect(valid);
        }
        for invalid in [
            "",
            "main",
            "--all",
            "refs/heads/",
            "refs/tags/v1",
            "refs/heads/a..b",
            "refs/heads/main@{1}",
            "refs/heads/main~1",
            "refs/heads/a b",
            "HEAD^",
        ] {
            assert!(validate_ref(invalid).is_err(), "{invalid:?}");
        }
        validate_sha(&"a".repeat(40)).expect("sha-1");
        validate_sha(&"0".repeat(64)).expect("sha-256");
        for invalid in ["abc123", "--all", &"g".repeat(40), &"a".repeat(41)] {
            assert!(validate_sha(invalid).is_err(), "{invalid:?}");
        }
    }
}
