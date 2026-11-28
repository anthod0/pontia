use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use pontia_core::error::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{WorkspaceGitService, command::GitCommand, diff::validate_repo_path};
use crate::{WorkspaceGitStatusView, git_status::refresh_workspace_git_status_now};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathSelection {
    #[serde(default)]
    all: bool,
    paths: Option<Vec<String>>,
}

impl PathSelection {
    fn paths(&self) -> Result<Vec<&str>> {
        match (self.all, &self.paths) {
            (true, None) => Ok(vec!["."]),
            (false, Some(paths)) => checked_paths(paths),
            _ => Err(Error::Domain("Provide paths or all: true".into())),
        }
    }
}

pub enum GitWrite {
    Stage(PathSelection),
    Unstage(PathSelection),
    Discard(Vec<String>),
    Commit { message: String, stage_all: bool },
    Pull,
    Push,
}

#[derive(Serialize)]
pub struct GitWriteResult {
    git_status: WorkspaceGitStatusView,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<CreatedCommit>,
}

#[derive(Serialize)]
struct CreatedCommit {
    sha: String,
    subject: String,
}

impl WorkspaceGitService {
    pub async fn write(&self, workspace_id: &str, operation: GitWrite) -> Result<GitWriteResult> {
        self.coordinator
            .mutate(workspace_id, || async {
                let mut repository = self.require_repository(workspace_id).await?;
                let command = &mut repository.command;
                if !matches!(operation, GitWrite::Discard(_)) {
                    command.enable_write_filters();
                }
                let mut commit = None;
                match operation {
                    GitWrite::Stage(selection) => {
                        with_paths(command, &["add", "-A", "--"], selection.paths()?).await?;
                    }
                    GitWrite::Unstage(selection) => {
                        with_paths(command, &["reset", "-q", "--"], selection.paths()?).await?;
                    }
                    GitWrite::Discard(paths) => discard(command, &paths).await?,
                    GitWrite::Commit { message, stage_all } => {
                        commit = Some(create_commit(command, &message, stage_all).await?);
                    }
                    GitWrite::Pull => {
                        command
                            .run_text(&[
                                "pull",
                                "--ff-only",
                                "--no-rebase",
                                "--no-autostash",
                                "--no-recurse-submodules",
                            ])
                            .await?;
                    }
                    GitWrite::Push => push(command).await?,
                }
                let git_status = refresh_workspace_git_status_now(
                    self.pool.clone(),
                    workspace_id,
                    command.dir(),
                )
                .await?;
                Ok(GitWriteResult { git_status, commit })
            })
            .await
    }
}

async fn create_commit(
    command: &GitCommand,
    message: &str,
    stage_all: bool,
) -> Result<CreatedCommit> {
    if message.trim().is_empty() || message.contains('\0') {
        return Err(Error::Domain(
            "A non-empty commit message is required".into(),
        ));
    }
    if stage_all
        && command
            .run_text(&["diff", "--cached", "--name-only", "-z"])
            .await?
            .is_empty()
    {
        command.run_text(&["add", "-A", "--", "."]).await?;
    }
    command
        .run_text(&["commit", "--quiet", "-m", message])
        .await?;
    let summary = command
        .run_text(&["log", "-1", "--format=%H%x00%s", "HEAD", "--"])
        .await?;
    let (sha, subject) = summary
        .trim_end_matches('\n')
        .split_once('\0')
        .ok_or_else(|| Error::Domain("Invalid Git commit summary".into()))?;
    Ok(CreatedCommit {
        sha: sha.into(),
        subject: subject.into(),
    })
}

fn checked_paths(paths: &[String]) -> Result<Vec<&str>> {
    if paths.is_empty() {
        return Err(Error::Domain("At least one path is required".into()));
    }
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for path in paths {
        validate_repo_path(path)?;
        if path
            .split('/')
            .any(|part| part.eq_ignore_ascii_case(".git"))
        {
            return Err(Error::Domain("Cannot modify Git metadata".into()));
        }
        if seen.insert(path) {
            selected.push(path.as_str());
        }
    }
    Ok(selected)
}

async fn with_paths(command: &GitCommand, args: &[&str], paths: Vec<&str>) -> Result<()> {
    let mut args = args.to_vec();
    args.extend(paths);
    command.run_text(&args).await?;
    Ok(())
}

async fn discard(command: &GitCommand, paths: &[String]) -> Result<()> {
    let paths = checked_paths(paths)?;
    let mut args = vec![
        "status",
        "--porcelain=v1",
        "-z",
        "--no-renames",
        "--untracked-files=all",
        "--",
    ];
    args.extend(&paths);
    let status = command.run(&args, 8 * 1024 * 1024, false).await?;
    if status.truncated {
        return Err(Error::Domain("Git status exceeds the discard limit".into()));
    }
    let entries: HashMap<_, _> = status
        .stdout
        .split(|byte| *byte == 0)
        .filter(|line| line.len() > 3)
        .map(|line| (&line[3..], &line[..2]))
        .collect();
    let mut restore = Vec::new();
    let mut remove = Vec::new();
    for path in paths {
        match entries.get(path.as_bytes()).copied() {
            Some(b"??") => remove.push(path),
            Some([b' ' | b'A' | b'M' | b'T', b'M' | b'T' | b'D']) => {
                check_restore_path(command.dir(), path)?;
                restore.push(path);
            }
            _ => return Err(Error::Domain(format!("{path:?} is not an unstaged file"))),
        }
    }
    if !restore.is_empty() {
        command.check_discard_filters(&restore).await?;
        with_paths(
            command,
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "restore",
                "--worktree",
                "--no-recurse-submodules",
                "--",
            ],
            restore,
        )
        .await?;
    }
    for path in remove {
        unlink_file(command.dir(), path)?;
    }
    Ok(())
}

// Restoring a deleted file may recreate its parents. A new file, directory
// or symlink blocking the checkout belongs to a separate working-tree change.
fn check_restore_path(root: &Path, path: &str) -> Result<()> {
    let mut current = root.to_path_buf();
    let mut parts = path.split('/').peekable();
    while let Some(part) = parts.next() {
        current.push(part);
        match current.symlink_metadata() {
            Ok(meta) if meta.is_dir() == parts.peek().is_some() => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
            Err(err) => return Err(err.into()),
            _ => {
                return Err(Error::Domain(format!(
                    "Cannot restore {path:?} over another path"
                )));
            }
        }
    }
    Ok(())
}

fn unlink_file(root: &Path, path: &str) -> std::io::Result<()> {
    use rustix::fs::{AtFlags, Mode, OFlags, open, openat, unlinkat};
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let absolute = root.join(path);
    let mut parent = open("/", flags, Mode::empty())?;
    for component in absolute.parent().expect("validated file path").components() {
        if let std::path::Component::Normal(directory) = component {
            parent = openat(&parent, directory, flags, Mode::empty())?;
        }
    }
    unlinkat(
        parent,
        absolute.file_name().expect("validated file path"),
        AtFlags::empty(),
    )?;
    Ok(())
}

async fn push(command: &GitCommand) -> Result<()> {
    let head = command
        .run_text(&["symbolic-ref", "--quiet", "HEAD"])
        .await?;
    let head = head.trim_end_matches('\n');
    if !head.starts_with("refs/heads/") {
        return Err(Error::Domain("Push requires a local branch".into()));
    }
    let remote = command
        .run_text(&["for-each-ref", "--format=%(push:remotename)", head])
        .await?;
    let remote = remote.trim_end_matches('\n');
    let remote = if remote.is_empty() { "origin" } else { remote };
    command
        .run_text(&[
            "push",
            "--no-force",
            "--no-follow-tags",
            "--recurse-submodules=no",
            "--",
            remote,
            &format!("{head}:{head}"),
        ])
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_selection_keeps_literal_names_and_rejects_unsafe_components() {
        let paths = ["-n", "*.rs", "dir/a\nb", "-n"].map(String::from);
        assert_eq!(checked_paths(&paths).unwrap(), ["-n", "*.rs", "dir/a\nb"]);
        for path in [
            ".",
            "..",
            "/file",
            "a/../b",
            ".git/index",
            "a/.GIT/config",
            "",
        ] {
            assert!(checked_paths(&[path.into()]).is_err());
        }
    }

    #[test]
    fn unlink_cannot_follow_a_replaced_parent_or_remove_a_directory() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        std::fs::write(root.path().join("keep"), "outside").unwrap();
        std::os::unix::fs::symlink(root.path(), repo.join("parent")).unwrap();
        assert!(unlink_file(&repo, "parent/keep").is_err());
        assert_eq!(std::fs::read(root.path().join("keep")).unwrap(), b"outside");
        std::fs::create_dir(repo.join("dir")).unwrap();
        assert!(unlink_file(&repo, "dir").is_err());
    }
}
