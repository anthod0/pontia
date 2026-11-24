import { getContext, setContext, untrack } from "svelte";
import type { LocalhostPreviewTarget } from "$lib/localhostPreview";
import { SidePanelOpen } from "$lib/sidePanelOpen.svelte";
import { PersistedState } from "$lib/storage";
import type { RecentFile } from "$lib/botRecentFiles";
import { createSessionRecentFiles, withSessionRecentFile } from "$lib/sessionRecentFiles";
import type { DeviceRuntime } from "../../../device/runtime";
import type { FilePreviewTarget } from "../filePreviewContext";
import { GitPanelState } from "../git/gitPanelState.svelte";

const panelTabs = ["files", "git"] as const;

/** A tab of the session side panel. */
export type SessionPanelTab = (typeof panelTabs)[number];

/** The Session the panel shows, registered by the session page. */
export type SessionPanelSource = {
  device: DeviceRuntime;
  sessionId: string;
  workspaceId: string | null;
};

/**
 * The right-hand side panel of session pages, with a Files tab for file previews and a Git tab
 * for the Workspace's changes. It outlives each session page, so it stays open across sessions,
 * and on wide screens across reloads. It reopens on the tab last used.
 */
export class SessionPanel {
  // The key comes from when the panel showed only Git; saved keys keep their names.
  #open = new SidePanelOpen("pontia.gitPanelOpen");
  #tab = new PersistedState<SessionPanelTab>("pontia.sessionPanelTab", "files", {
    parse: (saved) => panelTabs.find((tab) => tab === saved),
    stringify: (tab) => tab,
  });
  mobileEntry?: { sessionId: string; tab: SessionPanelTab };
  source = $state.raw<SessionPanelSource | null>(null);
  /** The last Session shown, kept while the next session page loads. */
  #last: SessionPanelSource | null = null;
  preview = $state.raw<LocalhostPreviewTarget | null>(null);
  /** The file previewed in the Files tab. */
  file = $state.raw<FilePreviewTarget | null>(null);
  /** What the Git tab shows for the Session's Workspace. */
  readonly git = new GitPanelState();
  #recentFiles = new Map<string, PersistedState<RecentFile[]>>();
  /** Set when the panel is opened by hand, telling it to take focus. Remounts leave focus where it is. */
  focusPending = $state(false);

  get open(): boolean {
    return this.#open.current;
  }

  get tab(): SessionPanelTab {
    return this.#tab.current;
  }

  set tab(tab: SessionPanelTab) {
    this.#tab.current = tab;
  }

  /**
   * Shows this Session until the returned function runs. Another Session closes its file, and
   * another Workspace what its Git tab had open. Callers may run it in an effect, so it does not track the panel
   * state it replaces.
   */
  attach(source: SessionPanelSource): () => void {
    untrack(() => {
      const last = this.#last;
      if (last?.device !== source.device || last.sessionId !== source.sessionId) {
        this.file = null;
        this.preview = null;
      }
      if (last?.device !== source.device || last.workspaceId !== source.workspaceId) {
        this.git.reset();
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
    if (this.open) this.close();
    else this.#show(true);
  }

  /** Opens the Git tab, or closes the panel when it already shows. */
  toggleGit(): void {
    if (this.open && !this.preview && this.tab === "git") {
      this.close();
      return;
    }
    this.preview = null;
    this.tab = "git";
    this.#show(true);
  }

  close(): void {
    this.#open.current = false;
    this.focusPending = false;
  }

  openPreview(target: LocalhostPreviewTarget) {
    this.preview = { ...target };
    this.#show(false);
  }

  /** Previews a file in the Files tab. This is where local file links in the chat open. */
  openFile(target: FilePreviewTarget): void {
    const recent = this.recentFiles(target.device);
    recent.current = withSessionRecentFile(recent.current, {
      sessionId: target.sessionId,
      path: target.path,
    });
    this.preview = null;
    this.file = target;
    this.tab = "files";
    this.#show(false);
  }

  /** The files opened before on this device, across Sessions. */
  recentFiles(device: DeviceRuntime): PersistedState<RecentFile[]> {
    let files = this.#recentFiles.get(device.id);
    if (!files) this.#recentFiles.set(device.id, (files = createSessionRecentFiles(device.id)));
    return files;
  }

  closeFile(): void {
    this.file = null;
  }

  #show(focus: boolean): void {
    this.#open.current = true;
    // Only a mounted panel takes focus; a page still loading would take it later, unasked.
    this.focusPending = focus && this.source !== null;
  }
}

const key = Symbol("session-panel");

export function provideSessionPanel(): SessionPanel {
  return setContext(key, new SessionPanel());
}

export function useSessionPanel(): SessionPanel | undefined {
  return getContext<SessionPanel | undefined>(key);
}
