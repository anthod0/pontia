import { getContext, setContext, untrack } from "svelte";
import { get, type Writable } from "svelte/store";
import { SidePanelOpen } from "$lib/sidePanelOpen.svelte";
import type { DeviceRuntime } from "../../../device/runtime";
import type { GitDiffTarget } from "../../../queries/workspaces";
import type { FilePreviewTarget } from "../filePreviewContext";

/** The Workspace whose changes the panel shows, registered by the session page. */
export type GitPanelSource = { device: DeviceRuntime; workspaceId: string };

/**
 * The Git panel shares the right-hand slot with the file preview, which takes the slot while
 * it is open. On wide screens it stays open or closed across sessions.
 */
export class GitPanel {
  #open = new SidePanelOpen("pontia.gitPanelOpen");
  #preview: Writable<FilePreviewTarget | null>;
  source = $state.raw<GitPanelSource | null>(null);
  /** The last Workspace shown, kept while the session page loads so an open diff survives. */
  #last: GitPanelSource | null = null;
  /** The file whose diff is shown; null shows the change list. */
  file = $state.raw<GitDiffTarget | null>(null);
  /** Increases each time the panel is opened on request, telling the panel to take focus. */
  focusRequest = $state(0);

  constructor(preview: Writable<FilePreviewTarget | null>) {
    this.#preview = preview;
  }

  get open(): boolean {
    return this.#open.current;
  }

  /**
   * Shows this Workspace's changes until the returned function runs. Callers may run it in an
   * effect, so it does not track the panel state it replaces.
   */
  attach(source: GitPanelSource): () => void {
    untrack(() => {
      const last = this.#last;
      if (last?.device !== source.device || last.workspaceId !== source.workspaceId) {
        this.file = null;
      }
      this.#last = source;
      this.source = source;
    });
    return () => {
      untrack(() => {
        if (this.source === source) this.source = null;
      });
    };
  }

  toggle(): void {
    if (this.open && !get(this.#preview)) this.close();
    else this.#show();
  }

  close(): void {
    this.#open.current = false;
    this.file = null;
  }

  /** Opens one file's diff. This is the entry point for opening a diff from elsewhere on the page. */
  openFile(file: GitDiffTarget): void {
    this.file = file;
    this.#show();
  }

  showList(): void {
    this.file = null;
  }

  #show(): void {
    this.#preview.set(null);
    this.#open.current = true;
    this.focusRequest++;
  }
}

const key = Symbol("git-panel");

export function provideGitPanel(preview: Writable<FilePreviewTarget | null>): GitPanel {
  return setContext(key, new GitPanel(preview));
}

export function useGitPanel(): GitPanel | undefined {
  return getContext<GitPanel | undefined>(key);
}
