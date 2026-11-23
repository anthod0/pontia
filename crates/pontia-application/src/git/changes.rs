use pontia_core::error::{Error, Result};
use serde::Serialize;
use std::{collections::HashMap, io::Read, path::Path};

use super::command::run_git_text;

/// Most files returned in one change list; `total` still counts every change.
pub const MAX_CHANGED_FILES: usize = 1000;
/// Untracked files larger than this are listed without a line count.
const UNTRACKED_COUNT_LIMIT: u64 = 1024 * 1024;
/// Bytes read in total to count untracked lines for one change list; later files get no count.
const UNTRACKED_COUNT_BUDGET: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChangeGroup {
    Conflicted,
    Staged,
    Unstaged,
    Untracked,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChangedFileView {
    /// Path relative to the repository root. Untracked directories end with `/`.
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<String>,
    pub group: ChangeGroup,
    /// One of `M`, `A`, `D`, `R`, `C`, `T`, `U` or `?`.
    pub status: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub binary: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitChangesView {
    /// `None` when the Workspace is not inside a Git work tree.
    pub repo_root: Option<String>,
    pub files: Vec<ChangedFileView>,
    pub total: usize,
    pub truncated: bool,
}

pub(crate) async fn list_changes(repo_root: &Path) -> Result<(Vec<ChangedFileView>, usize)> {
    let status = run_git_text(
        repo_root,
        &["status", "--porcelain=v2", "-z", "--untracked-files=normal"],
    )
    .await?;
    let mut files = parse_status(&status)?;
    // Order matches the panel: conflicted, staged, unstaged, untracked.
    files.sort_by_key(|file| file.group as u8);
    let total = files.len();
    files.truncate(MAX_CHANGED_FILES);

    let staged = run_git_text(
        repo_root,
        &[
            "diff",
            "--cached",
            "--numstat",
            "-z",
            "-M",
            "--no-ext-diff",
            "--no-textconv",
        ],
    )
    .await?;
    let unstaged = run_git_text(
        repo_root,
        &[
            "diff",
            "--numstat",
            "-z",
            "-M",
            "--no-ext-diff",
            "--no-textconv",
        ],
    )
    .await?;
    let staged = parse_numstat(&staged);
    let unstaged = parse_numstat(&unstaged);

    let root = repo_root.to_path_buf();
    let files = tokio::task::spawn_blocking(move || {
        let mut budget = UNTRACKED_COUNT_BUDGET;
        for file in &mut files {
            let stat = match file.group {
                ChangeGroup::Staged => staged.get(&file.path).copied(),
                ChangeGroup::Unstaged => unstaged.get(&file.path).copied(),
                ChangeGroup::Untracked => count_untracked(&root, &file.path, &mut budget),
                ChangeGroup::Conflicted => None,
            };
            match stat {
                Some(Numstat::Text(additions, deletions)) => {
                    file.additions = Some(additions);
                    file.deletions = Some(deletions);
                }
                Some(Numstat::Binary) => file.binary = true,
                None => {}
            }
        }
        files
    })
    .await
    .map_err(|err| Error::Domain(format!("git change count failed: {err}")))?;
    Ok((files, total))
}

fn parse_status(output: &str) -> Result<Vec<ChangedFileView>> {
    let mut files = Vec::new();
    let mut records = output.split('\0').filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        let invalid = || Error::Domain(format!("unexpected git status record: {record}"));
        let entry = |path: &str, group, status: char| ChangedFileView {
            path: path.to_string(),
            orig_path: None,
            group,
            status: status.to_string(),
            additions: None,
            deletions: None,
            binary: false,
        };
        let (kind, fields) = match record.as_bytes()[0] {
            b'?' => {
                let path = record.get(2..).ok_or_else(invalid)?;
                files.push(entry(path, ChangeGroup::Untracked, '?'));
                continue;
            }
            b'u' => {
                let path = record.splitn(11, ' ').nth(10).ok_or_else(invalid)?;
                files.push(entry(path, ChangeGroup::Conflicted, 'U'));
                continue;
            }
            b'1' => (1, 9),
            b'2' => (2, 10),
            _ => return Err(invalid()),
        };
        let fields = record.splitn(fields, ' ').collect::<Vec<_>>();
        let (Some(xy), Some(path)) = (fields.get(1), fields.last()) else {
            return Err(invalid());
        };
        let orig_path = if kind == 2 {
            Some(records.next().ok_or_else(invalid)?)
        } else {
            None
        };
        let mut xy = xy.chars();
        let (x, y) = (
            xy.next().ok_or_else(invalid)?,
            xy.next().ok_or_else(invalid)?,
        );
        // A rename belongs to the side whose status is R or C; the other side changes the new path.
        for (code, group) in [(x, ChangeGroup::Staged), (y, ChangeGroup::Unstaged)] {
            if code == '.' {
                continue;
            }
            let mut file = entry(path, group, code);
            if matches!(code, 'R' | 'C') {
                file.orig_path = orig_path.map(str::to_string);
            }
            files.push(file);
        }
    }
    Ok(files)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Numstat {
    Text(u64, u64),
    Binary,
}

/// Parses `git diff --numstat -z`, keyed by the new path. A rename record has an empty path
/// field followed by the old and new paths as separate NUL-terminated fields.
pub(crate) fn parse_numstat(output: &str) -> HashMap<String, Numstat> {
    let mut stats = HashMap::new();
    let mut fields = output.split('\0');
    while let Some(record) = fields.next() {
        let mut parts = record.splitn(3, '\t');
        let (Some(additions), Some(deletions), Some(path)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let path = if path.is_empty() {
            fields.next();
            match fields.next() {
                Some(path) => path,
                None => break,
            }
        } else {
            path
        };
        let stat = match (additions.parse(), deletions.parse()) {
            (Ok(additions), Ok(deletions)) => Numstat::Text(additions, deletions),
            _ => Numstat::Binary,
        };
        stats.insert(path.to_string(), stat);
    }
    stats
}

/// Counts lines of a small untracked text file while `budget` bytes remain. Symbolic links
/// and directories are not read.
fn count_untracked(root: &Path, path: &str, budget: &mut u64) -> Option<Numstat> {
    if path.ends_with('/') {
        return None;
    }
    let full = root.join(path);
    let metadata = std::fs::symlink_metadata(&full).ok()?;
    if !metadata.is_file() || metadata.len() > UNTRACKED_COUNT_LIMIT || metadata.len() > *budget {
        return None;
    }
    *budget -= metadata.len();
    let mut content = Vec::new();
    std::fs::File::open(&full)
        .ok()?
        .take(UNTRACKED_COUNT_LIMIT)
        .read_to_end(&mut content)
        .ok()?;
    if content.contains(&0) {
        return Some(Numstat::Binary);
    }
    let mut lines = content.iter().filter(|byte| **byte == b'\n').count() as u64;
    if content.last().is_some_and(|byte| *byte != b'\n') {
        lines += 1;
    }
    Some(Numstat::Text(lines, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_records_into_groups() {
        let output = "1 MM N... 100644 100644 100644 a b src/a b.rs\x001 .D N... 100644 100644 000000 a a gone.txt\0u UU N... 100644 100644 100644 100644 a b c conflict.txt\0? new dir/\0? notes.txt\0";
        let files = parse_status(output).expect("parse");
        let summary = files
            .iter()
            .map(|file| (file.path.as_str(), file.group, file.status.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            vec![
                ("src/a b.rs", ChangeGroup::Staged, "M"),
                ("src/a b.rs", ChangeGroup::Unstaged, "M"),
                ("gone.txt", ChangeGroup::Unstaged, "D"),
                ("conflict.txt", ChangeGroup::Conflicted, "U"),
                ("new dir/", ChangeGroup::Untracked, "?"),
                ("notes.txt", ChangeGroup::Untracked, "?"),
            ]
        );
    }

    #[test]
    fn parses_staged_rename_with_original_path() {
        let files =
            parse_status("2 RM N... 100644 100644 100644 a b R100 new name.rs\0old name.rs\0")
                .expect("parse");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].group, ChangeGroup::Staged);
        assert_eq!(files[0].status, "R");
        assert_eq!(files[0].path, "new name.rs");
        assert_eq!(files[0].orig_path.as_deref(), Some("old name.rs"));
        assert_eq!(files[1].group, ChangeGroup::Unstaged);
        assert_eq!(files[1].orig_path, None);
    }

    #[test]
    fn rejects_unexpected_status_records() {
        assert!(parse_status("! ignored.txt\0").is_err());
    }

    #[test]
    fn parses_numstat_including_binary() {
        let stats = parse_numstat("3\t1\tsrc/lib.rs\0-\t-\timage.png\x001\t0\t\0old.rs\0new.rs\0");
        assert_eq!(stats.get("new.rs"), Some(&Numstat::Text(1, 0)));
        assert_eq!(stats.get("src/lib.rs"), Some(&Numstat::Text(3, 1)));
        assert_eq!(stats.get("image.png"), Some(&Numstat::Binary));
    }
}
