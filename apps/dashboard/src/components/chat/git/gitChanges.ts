import type { GitChangedFile, GitChangeGroup, GitDiffLine } from "../../../api/types";
import type { MessageKey } from "$lib/i18n";

/** Group order, symbol and color, matching the composer's git summary. */
export const gitChangeGroups: {
  group: GitChangeGroup;
  label: MessageKey;
  symbol: string;
  tone: string;
}[] = [
  { group: "conflicted", label: "git.conflicted", symbol: "!", tone: "text-destructive" },
  { group: "staged", label: "git.staged", symbol: "+", tone: "text-success" },
  { group: "unstaged", label: "git.unstaged", symbol: "~", tone: "text-warning" },
  { group: "untracked", label: "git.untracked", symbol: "?", tone: "text-aqua" },
];

const statusTones: Record<string, string> = {
  M: "text-warning",
  T: "text-warning",
  A: "text-success",
  C: "text-success",
  D: "text-destructive",
  U: "text-destructive",
  R: "text-primary",
  "?": "text-aqua",
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
    .map((group) => ({ ...group, files: files.filter((file) => file.group === group.group) }))
    .filter((group) => group.files.length > 0);
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
 * The directory shown after a file name. A move shows `old dir → new dir`; when the name
 * changed too, the old side is the whole old path so the old name stays visible. The
 * repository root shows as `.`.
 */
export function changeDirLabel(file: Pick<GitChangedFile, "path" | "orig_path">): string {
  const to = splitChangePath(file.path);
  if (!file.orig_path) return to.dir;
  const from = splitChangePath(file.orig_path);
  const old = from.name === to.name ? from.dir || "." : file.orig_path;
  return `${old} → ${to.dir || "."}`;
}

/** The first characters of a commit name, as `git log --oneline` shows it. */
export function shortSha(sha: string): string {
  return sha.slice(0, 7);
}

/** Up and Down move focus between the `[data-git-row]` buttons inside `list`. */
export function moveRowFocus(event: KeyboardEvent, list: HTMLElement): void {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  const rows = [...list.querySelectorAll<HTMLElement>("[data-git-row]")];
  if (!rows.length) return;
  event.preventDefault();
  const index = rows.indexOf(document.activeElement as HTMLElement);
  const step = event.key === "ArrowDown" ? 1 : -1;
  const next = index < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, index + step));
  rows[next]!.focus();
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
