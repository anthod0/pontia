import type { GitChangedFile, GitChangeGroup, WorkspaceGitStatusView } from "../../../api/types";
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

export function parentDir(path: string): string {
  return splitChangePath(path).dir;
}

/** The number of changed file entries in a Git status summary. */
export function gitStatusChangeCount(status: WorkspaceGitStatusView | undefined): number {
  if (!status || status.state !== "observed") return 0;
  return (
    status.staged_count + status.unstaged_count + status.untracked_count + status.conflicted_count
  );
}

export function changeCountLabel(count: number): string {
  return count > 99 ? "99+" : String(count);
}

export function sameChange(
  a: { path: string; group: GitChangeGroup },
  b: { path: string; group: GitChangeGroup },
) {
  return a.path === b.path && a.group === b.group;
}
