use super::{changes::ChangeGroup, command::GitCommand};
use pontia_core::error::{Error, Result};
use serde::Serialize;

/// Largest diff output read for one file.
pub(crate) const MAX_DIFF_BYTES: usize = 512 * 1024;
/// Most diff lines returned for one file.
const MAX_DIFF_LINES: usize = 2000;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Add,
    Delete,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiffLineView {
    pub kind: DiffLineKind,
    pub old: Option<u64>,
    pub new: Option<u64>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiffHunkView {
    pub header: String,
    pub lines: Vec<DiffLineView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitDiffView {
    pub path: String,
    pub group: ChangeGroup,
    pub binary: bool,
    pub hunks: Vec<DiffHunkView>,
    /// The diff exceeded the server limit and only its beginning is returned.
    pub truncated: bool,
}

/// Validates a repository-relative path received from a client.
pub(crate) fn validate_repo_path(path: &str) -> Result<()> {
    let valid = !path.contains('\0')
        && !path.contains('\\')
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."));
    if valid {
        Ok(())
    } else {
        Err(Error::Domain(format!("invalid repository path: {path:?}")))
    }
}

pub(crate) async fn file_diff(
    command: &GitCommand,
    path: &str,
    orig_path: Option<&str>,
    group: ChangeGroup,
) -> Result<GitDiffView> {
    validate_repo_path(path)?;
    if let Some(orig_path) = orig_path {
        // Only a staged rename or copy pairs two paths; status reports it with --cached.
        if group != ChangeGroup::Staged {
            return Err(Error::Domain(
                "orig_path is only valid for staged changes".to_string(),
            ));
        }
        validate_repo_path(orig_path)?;
    }
    let common = [
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ];
    let output = match group {
        ChangeGroup::Untracked => {
            ensure_untracked(command, path).await?;
            let mut args = vec!["diff", "--no-index"];
            args.extend(common);
            args.extend(["--", "/dev/null", path]);
            command.run(&args, MAX_DIFF_BYTES, true).await?
        }
        ChangeGroup::Staged | ChangeGroup::Unstaged | ChangeGroup::Conflicted => {
            let mut args = vec!["diff", "-M"];
            match group {
                ChangeGroup::Staged => args.push("--cached"),
                // Conflicts are shown against HEAD so the markers read as one unified diff.
                ChangeGroup::Conflicted => args.push("HEAD"),
                _ => {}
            }
            args.extend(common);
            args.push("--");
            args.extend(orig_path);
            args.push(path);
            command.run(&args, MAX_DIFF_BYTES, false).await?
        }
    };
    let mut diff = parse_unified_diff(&String::from_utf8_lossy(&output.stdout));
    diff.truncated |= output.truncated;
    Ok(GitDiffView {
        path: path.to_string(),
        group,
        binary: diff.binary,
        hunks: diff.hunks,
        truncated: diff.truncated,
    })
}

/// Only a path git reports as an untracked file is read, so `--no-index` never follows a
/// symbolic link to content outside the repository.
async fn ensure_untracked(command: &GitCommand, path: &str) -> Result<()> {
    let status = command
        .run_text(&[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--",
            path,
        ])
        .await?;
    let expected = format!("? {path}");
    if status.split('\0').any(|record| record == expected) {
        Ok(())
    } else {
        Err(Error::NotFound(format!("{path} is not an untracked file")))
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ParsedDiff {
    pub binary: bool,
    pub hunks: Vec<DiffHunkView>,
    pub truncated: bool,
}

pub(crate) fn parse_unified_diff(output: &str) -> ParsedDiff {
    let mut parsed = ParsedDiff::default();
    let (mut old, mut new) = (0, 0);
    // Lines still expected on each side of the current hunk; a hunk ends when both reach zero.
    let (mut old_left, mut new_left) = (0u64, 0u64);
    let mut line_count = 0;
    for line in output.lines() {
        if old_left == 0 && new_left == 0 {
            if let Some(header) = line.strip_prefix("@@ ")
                && let Some(range) = hunk_range(header)
            {
                (old, old_left, new, new_left) = range;
                parsed.hunks.push(DiffHunkView {
                    header: line.to_string(),
                    lines: Vec::new(),
                });
            } else if parsed.hunks.is_empty()
                && (line.starts_with("Binary files ") || line == "GIT binary patch")
            {
                parsed.binary = true;
            }
            continue;
        }
        let (kind, text) = match line.as_bytes().first() {
            Some(b' ') => (DiffLineKind::Context, &line[1..]),
            Some(b'+') => (DiffLineKind::Add, &line[1..]),
            Some(b'-') => (DiffLineKind::Delete, &line[1..]),
            // A blank context line can lose its leading space in some outputs.
            None => (DiffLineKind::Context, ""),
            // "\ No newline at end of file".
            _ => continue,
        };
        if line_count == MAX_DIFF_LINES {
            parsed.truncated = true;
            break;
        }
        line_count += 1;
        let (old_line, new_line) = match kind {
            DiffLineKind::Context => (Some(old), Some(new)),
            DiffLineKind::Add => (None, Some(new)),
            DiffLineKind::Delete => (Some(old), None),
        };
        if kind != DiffLineKind::Add {
            old += 1;
            old_left = old_left.saturating_sub(1);
        }
        if kind != DiffLineKind::Delete {
            new += 1;
            new_left = new_left.saturating_sub(1);
        }
        let hunk = parsed.hunks.last_mut().expect("a hunk is open");
        hunk.lines.push(DiffLineView {
            kind,
            old: old_line,
            new: new_line,
            text: text.to_string(),
        });
    }
    parsed
}

/// Reads `(old start, old count, new start, new count)` from `-a,b +c,d @@ ...`; a missing
/// count means one line.
fn hunk_range(header: &str) -> Option<(u64, u64, u64, u64)> {
    let mut parts = header.split(' ');
    let range = |part: Option<&str>, sign: char| -> Option<(u64, u64)> {
        let mut numbers = part?.strip_prefix(sign)?.split(',');
        let start = numbers.next()?.parse().ok()?;
        let count = numbers.next().map_or(Some(1), |count| count.parse().ok())?;
        Some((start, count))
    };
    let (old_start, old_count) = range(parts.next(), '-')?;
    let (new_start, new_count) = range(parts.next(), '+')?;
    Some((old_start, old_count, new_start, new_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_outside_the_repository() {
        for path in [
            "",
            "/etc/passwd",
            "../x",
            "a/../../x",
            "a/./b",
            "a\\b",
            "a\0b",
        ] {
            assert!(validate_repo_path(path).is_err(), "{path:?}");
        }
        validate_repo_path("src/lib.rs").expect("valid");
        validate_repo_path(".github/workflows/ci.yml").expect("valid");
    }

    #[test]
    fn parses_hunks_with_line_numbers() {
        let parsed = parse_unified_diff(
            "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@ fn main\n a\n-b\n+B\n c\n\\ No newline at end of file\n@@ -10 +10,2 @@\n x\n+y\n",
        );
        assert!(!parsed.binary);
        assert_eq!(parsed.hunks.len(), 2);
        let lines = &parsed.hunks[0].lines;
        assert_eq!(parsed.hunks[0].header, "@@ -1,3 +1,3 @@ fn main");
        assert_eq!(
            lines
                .iter()
                .map(|line| (line.kind, line.old, line.new, line.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (DiffLineKind::Context, Some(1), Some(1), "a"),
                (DiffLineKind::Delete, Some(2), None, "b"),
                (DiffLineKind::Add, None, Some(2), "B"),
                (DiffLineKind::Context, Some(3), Some(3), "c"),
            ]
        );
        assert_eq!(parsed.hunks[1].lines[1].new, Some(11));
    }

    #[test]
    fn ends_each_hunk_at_its_line_counts() {
        let parsed = parse_unified_diff(
            "@@ -1 +1 @@\n-a\n+b\ndiff --git a/g b/g\n--- a/g\n+++ b/g\n@@ -0,0 +1 @@\n+c\n",
        );
        assert_eq!(parsed.hunks.len(), 2);
        assert_eq!(parsed.hunks[0].lines.len(), 2);
        assert_eq!(parsed.hunks[1].lines[0].text, "c");
    }

    #[test]
    fn detects_binary_diff() {
        let parsed = parse_unified_diff(
            "diff --git a/b.bin b/b.bin\nindex 1..2 100644\nBinary files a/b.bin and b/b.bin differ\n",
        );
        assert!(parsed.binary);
        assert!(parsed.hunks.is_empty());
    }

    #[test]
    fn truncates_long_diffs() {
        let body = "+x\n".repeat(MAX_DIFF_LINES + 5);
        let parsed = parse_unified_diff(&format!("@@ -0,0 +1,{} @@\n{body}", MAX_DIFF_LINES + 5));
        assert!(parsed.truncated);
        assert_eq!(parsed.hunks[0].lines.len(), MAX_DIFF_LINES);
    }
}
