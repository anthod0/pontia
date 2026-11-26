import type { GitChangedFile, GitChangeGroup, GitDiffLine } from "../../../api/types";
import type { MessageKey } from "$lib/i18n";

/** Display order for change groups. */
export const gitChangeGroups: {
  group: Exclude<GitChangeGroup, "untracked">;
  label: MessageKey;
}[] = [
  { group: "conflicted", label: "git.conflicted" },
  { group: "staged", label: "git.staged" },
  { group: "unstaged", label: "git.unstaged" },
];

const statusTones: Record<string, string> = {
  M: "text-git-modified",
  T: "text-git-modified",
  A: "text-git-added",
  C: "text-git-added",
  D: "text-git-deleted",
  U: "text-git-conflict",
  R: "text-git-renamed",
  "?": "text-git-added",
};

/** What a diff view draws, for an uncommitted change or a file in a commit. */
export type GitDiffContent = {
  binary: boolean;
  hunks: { header: string; lines: GitDiffLine[] }[];
  truncated: boolean;
};

export function countDiffLines(diff: GitDiffContent, kind: GitDiffLine["kind"]): number {
  return diff.hunks.reduce(
    (total, hunk) => total + hunk.lines.filter((line) => line.kind === kind).length,
    0,
  );
}

export function gitStatusTone(status: string): string {
  return statusTones[status] ?? "text-muted-foreground";
}

export function groupChanges(files: GitChangedFile[]) {
  return gitChangeGroups
    .map((group) => ({
      ...group,
      files: files.filter(
        (file) =>
          file.group === group.group || (group.group === "unstaged" && file.group === "untracked"),
      ),
    }))
    .filter((group) => group.files.length > 0);
}

export function displayChangeGroup(group: GitChangeGroup) {
  return gitChangeGroups.find(
    (item) => item.group === (group === "untracked" ? "unstaged" : group),
  )!;
}

export type WordHighlight = { start: number; end: number };

/** Finds the changed middle of adjacent deleted/added lines when most of the line is unchanged. */
export function wordHighlights(hunks: GitDiffContent["hunks"]): Map<GitDiffLine, WordHighlight> {
  const result = new Map<GitDiffLine, WordHighlight>();
  for (const hunk of hunks) {
    for (let index = 0; index < hunk.lines.length - 1; index += 1) {
      const deleted = hunk.lines[index]!;
      const added = hunk.lines[index + 1]!;
      if (deleted.kind !== "delete" || added.kind !== "add") continue;
      let prefix = 0;
      const limit = Math.min(deleted.text.length, added.text.length);
      while (prefix < limit && deleted.text[prefix] === added.text[prefix]) prefix += 1;
      let suffix = 0;
      while (
        suffix < limit - prefix &&
        deleted.text[deleted.text.length - 1 - suffix] ===
          added.text[added.text.length - 1 - suffix]
      ) {
        suffix += 1;
      }
      const deletedEnd = deleted.text.length - suffix;
      const addedEnd = added.text.length - suffix;
      const changed = Math.max(deletedEnd - prefix, addedEnd - prefix);
      const longest = Math.max(deleted.text.length, added.text.length);
      if (!changed || !longest || changed / longest > 0.7) continue;
      result.set(deleted, { start: prefix, end: deletedEnd });
      result.set(added, { start: prefix, end: addedEnd });
    }
  }
  return result;
}

/** Splits a repository path into the file name and its directory; directories keep their `/`. */
export function splitChangePath(path: string): { name: string; dir: string } {
  const trimmed = path.endsWith("/") ? path.slice(0, -1) : path;
  const slash = trimmed.lastIndexOf("/");
  return slash < 0
    ? { name: path, dir: "" }
    : { name: path.slice(slash + 1), dir: trimmed.slice(0, slash) };
}

/**
 * The directory shown after a file name. A rename in place shows its old name; a move
 * shows `old → new`. The repository root shows as `.`.
 */
export function changeDirLabel(file: Pick<GitChangedFile, "path" | "orig_path">): string {
  const to = splitChangePath(file.path);
  if (!file.orig_path) return to.dir;
  const from = splitChangePath(file.orig_path);
  if (from.dir === to.dir && from.name !== to.name) return `← ${from.name}`;
  const old = from.name === to.name ? from.dir || "." : file.orig_path;
  return `${old} → ${to.dir || "."}`;
}

/** The first characters of a commit name, as `git log --oneline` shows it. */
export function shortSha(sha: string): string {
  return sha.slice(0, 7);
}

/** Up and Down move focus between the `[data-git-row]` controls inside `list`. */
export function moveRowFocus(event: KeyboardEvent, list: HTMLElement, activate = false): void {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  const rows = [...list.querySelectorAll<HTMLElement>("[data-git-row]")];
  if (!rows.length) return;
  event.preventDefault();
  const index = rows.indexOf(document.activeElement as HTMLElement);
  const step = event.key === "ArrowDown" ? 1 : -1;
  const next = index < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, index + step));
  const row = rows[next]!;
  row.focus();
  if (activate && row instanceof HTMLButtonElement) row.click();
}

/** Focuses the row with `key` inside `root`, once the list it belongs to has rendered. */
export function focusRow(root: HTMLElement | undefined, key: string): void {
  queueMicrotask(() => {
    const row = [...(root?.querySelectorAll<HTMLElement>("[data-git-row]") ?? [])].find(
      (row) => row.dataset.gitRow === key,
    );
    row?.focus();
  });
}

export function changeKey(file: { path: string; group: GitChangeGroup }): string {
  return `${file.group}\0${file.path}`;
}

export function sameChange(
  a: { path: string; group: GitChangeGroup },
  b: { path: string; group: GitChangeGroup },
) {
  return a.path === b.path && a.group === b.group;
}
