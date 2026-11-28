import type { GitCommitFileTarget, GitDiffTarget } from "../../../queries/workspaces";

/**
 * What one Workspace's Git tab shows. Choosing a branch shows its history.
 */
export class GitPanelState {
  changesOpen = $state(true);
  historyOpen = $state(true);
  /** The uncommitted file whose diff shows. */
  diff = $state.raw<GitDiffTarget | null>(null);
  /** The branch ref whose history shows; null follows the checked-out branch. */
  ref = $state<string | null>(null);
  /** The expanded commit in a narrow panel, or the selected commit in a wide panel. */
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

  /** Forgets what showed for the previous Workspace. */
  reset(): void {
    this.changesOpen = true;
    this.historyOpen = true;
    this.diff = null;
    this.ref = null;
    this.commit = null;
    this.commitFile = null;
  }
}
