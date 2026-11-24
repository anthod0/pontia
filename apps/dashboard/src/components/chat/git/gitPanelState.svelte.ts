import type { GitCommitFileTarget, GitDiffTarget } from "../../../queries/workspaces";

/** The two lists of a Git tab: uncommitted changes, or the commit history of a branch. */
export type GitView = "changes" | "history";

/**
 * What one Workspace's Git tab shows. Everything here only reads the repository; choosing a
 * branch shows its history and never checks it out.
 */
export class GitPanelState {
  view = $state<GitView>("changes");
  /** The uncommitted file whose diff shows; null shows the change list. */
  diff = $state.raw<GitDiffTarget | null>(null);
  /** The branch ref whose history shows; null follows the checked-out branch. */
  ref = $state<string | null>(null);
  /** The commit whose details show; null shows the history list. */
  commit = $state<string | null>(null);
  /** The file of `commit` whose diff shows. */
  commitFile = $state.raw<GitCommitFileTarget | null>(null);

  openCommit(sha: string): void {
    this.commit = sha;
    this.commitFile = null;
  }

  chooseRef(ref: string): void {
    this.ref = ref;
    this.commit = null;
    this.commitFile = null;
  }

  /** Forgets what showed for the previous Workspace; the chosen list stays. */
  reset(): void {
    this.diff = null;
    this.ref = null;
    this.commit = null;
    this.commitFile = null;
  }
}
