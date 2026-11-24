import { translate } from "$lib/i18n";
import { useDeviceRuntime } from "../../device/context";
import type {
  WorkspaceAvailability,
  WorkspaceDirectoryEntryView,
  WorkspaceView,
} from "../../api/types";

export function createWorkspaceBrowser(initialView: "active" | "directory") {
  const device = useDeviceRuntime();
  const {
    createDeleteWorkspaceMutation,
    createRegisterWorkspaceMutation,
    createRenameWorkspaceMutation,
    createWorkspaceRootEntriesQuery,
    createWorkspaceRootsQuery,
    createWorkspacesQuery,
  } = device.workspaceQueries;

  type BrowserLocation =
    | { view: "active" }
    | { view: "unavailable" }
    | { view: "directory"; rootId: string; path: string };

  let rootId = $state("");
  let browsePath = $state("");
  let openedPath = $state("");
  const workspacesQuery = createWorkspacesQuery();
  const rootsQuery = createWorkspaceRootsQuery();
  const entriesQuery = createWorkspaceRootEntriesQuery(
    () => rootId,
    () => openedPath,
  );
  const registerWorkspaceMutation = createRegisterWorkspaceMutation();
  const renameWorkspaceMutation = createRenameWorkspaceMutation();
  const deleteWorkspaceMutation = createDeleteWorkspaceMutation();
  let registeringPath = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let deletingWorkspaceId = $state<string | null>(null);
  let renameError = $state<string | null>(null);
  let renamingWorkspace = $state<WorkspaceView | null>(null);
  let renamingWorkspaceName = $state("");
  let renameWorkspaceDialogOpen = $state(false);
  let savingRename = $state(false);
  let showHiddenDirectories = $state(false);
  let view = $state<BrowserLocation["view"]>(initialView);
  let workspaces = $derived(workspacesQuery.data ?? []);
  let activeWorkspaces = $derived(workspaces.filter((workspace) => workspace.state === "active"));
  let unavailableWorkspaces = $derived(
    activeWorkspaces.filter((workspace) => workspace.availability.status !== "available"),
  );
  let workspaceRoots = $derived(rootsQuery.data ?? []);
  let listing = $derived(entriesQuery.data ?? null);
  let selectedRoot = $derived(workspaceRoots.find((root) => root.root_id === rootId) ?? null);
  let currentWorkspace = $derived(
    listing ? workspaceForCanonicalPath(listing.canonical_path) : null,
  );
  let visibleEntries = $derived(
    (listing?.entries ?? [])
      .filter((entry) => showHiddenDirectories || !entry.name.startsWith("."))
      .sort(
        (left, right) =>
          Number(right.is_workspace) - Number(left.is_workspace) ||
          left.name.localeCompare(right.name),
      ),
  );
  let hiddenEntryCount = $derived((listing?.entries.length ?? 0) - visibleEntries.length);
  let breadcrumbs = $derived(pathBreadcrumbs(browsePath));
  let parentPath = $derived(
    browsePath ? browsePath.split("/").filter(Boolean).slice(0, -1).join("/") : null,
  );
  let browserLoadError = $derived(rootsQuery.error ?? workspacesQuery.error);
  let workspacesLoading = $derived(workspacesQuery.isPending);
  let actionsDisabled = $derived(
    workspacesLoading || registeringPath !== null || deletingWorkspaceId !== null,
  );
  $effect(() => {
    if (!rootId && workspaceRoots.length) rootId = workspaceRoots[0].root_id;
  });
  $effect(() => {
    if (initialView === "directory" && view === "unavailable" && !unavailableWorkspaces.length)
      view = "active";
  });
  $effect(() => {
    if (listing && listing.root_id === rootId && listing.path !== browsePath)
      browsePath = listing.path;
  });
  $effect(() => {
    if (!renameWorkspaceDialogOpen && renamingWorkspace && !savingRename) {
      renamingWorkspace = null;
      renamingWorkspaceName = "";
    }
  });

  async function retryDirectory(): Promise<void> {
    await entriesQuery.refetch();
  }

  async function refreshAll(): Promise<void> {
    await Promise.all([workspacesQuery.refetch(), rootsQuery.refetch()]);
    if (rootId) await entriesQuery.refetch();
  }

  function pathBreadcrumbs(path: string): { label: string; path: string }[] {
    const segments = path.split("/").filter(Boolean);
    return [
      { label: selectedRoot?.label ?? translate("workspaces.root"), path: "" },
      ...segments.map((segment, index) => ({
        label: segment,
        path: segments.slice(0, index + 1).join("/"),
      })),
    ];
  }

  function navigate(location: BrowserLocation): void {
    view = location.view;
    if (location.view !== "directory") return;
    rootId = location.rootId;
    loadPath(location.path);
  }

  function openRoot(nextRootId: string): void {
    navigate({ view: "directory", rootId: nextRootId, path: "" });
  }

  function openPath(path: string): void {
    navigate({ view: "directory", rootId, path });
  }

  function workspaceLocation(workspace: WorkspaceView): BrowserLocation | null {
    const availability = workspace.availability;
    return availability.status === "available"
      ? { view: "directory", rootId: availability.root_id, path: availability.path }
      : null;
  }

  function workspaceAvailabilityLabel(availability: WorkspaceAvailability): string {
    if (availability.status === "available") return translate("common.available");
    const labels = {
      outside_root: translate("workspaces.outsideRoot"),
      missing_directory: translate("workspaces.missingDirectory"),
      not_directory: translate("workspaces.notDirectory"),
      permission_denied: translate("workspaces.permissionDenied"),
      root_unavailable: translate("workspaces.rootUnavailable"),
      check_failed: translate("workspaces.cannotCheck"),
    };
    return labels[availability.reason];
  }

  function loadPath(path: string): void {
    if (!rootId) return;
    openedPath = path;
    browsePath = path;
  }

  function canonicalPathForEntry(entry: WorkspaceDirectoryEntryView): string | null {
    if (!selectedRoot?.canonical_path) return null;
    const rootPath = selectedRoot.canonical_path.replace(/\/+$/, "");
    return entry.path.trim() ? `${rootPath}/${entry.path}` : rootPath;
  }

  function workspaceForCanonicalPath(canonicalPath: string): WorkspaceView | null {
    return (
      workspaces.find(
        (workspace) =>
          workspace.canonical_path === canonicalPath || workspace.display_path === canonicalPath,
      ) ?? null
    );
  }

  function workspaceForEntry(entry: WorkspaceDirectoryEntryView): WorkspaceView | null {
    const canonicalPath = canonicalPathForEntry(entry);
    return canonicalPath ? workspaceForCanonicalPath(canonicalPath) : null;
  }

  function workspaceName(path: string): string {
    const segments = path.split("/").filter(Boolean);
    return segments.at(-1) ?? selectedRoot?.label ?? translate("workspace.title");
  }

  async function togglePath(path: string, workspace: WorkspaceView | null): Promise<void> {
    if (workspace) {
      await deleteRegisteredWorkspace(workspace.workspace_id);
      return;
    }
    if (!rootId || registeringPath !== null) return;
    registeringPath = path;
    actionError = null;
    try {
      await registerWorkspaceMutation.mutateAsync({
        root_id: rootId,
        path,
        name: workspaceName(path),
      });
    } catch (error) {
      actionError = errorMessage(error);
    } finally {
      registeringPath = null;
    }
  }

  function startRenamingWorkspace(workspace: WorkspaceView): void {
    renameError = null;
    renamingWorkspace = workspace;
    renamingWorkspaceName = workspace.name ?? workspace.display_path;
    renameWorkspaceDialogOpen = true;
  }

  function cancelRenamingWorkspace(): void {
    renameWorkspaceDialogOpen = false;
    renamingWorkspace = null;
    renamingWorkspaceName = "";
  }

  async function confirmRenameWorkspace(): Promise<void> {
    if (!renamingWorkspace || savingRename) return;
    savingRename = true;
    renameError = null;
    try {
      await renameWorkspaceMutation.mutateAsync({
        workspaceId: renamingWorkspace.workspace_id,
        input: { name: renamingWorkspaceName.trim() || null },
      });
      renameWorkspaceDialogOpen = false;
      renamingWorkspace = null;
      renamingWorkspaceName = "";
    } catch (error) {
      renameError = errorMessage(error);
    } finally {
      savingRename = false;
    }
  }

  async function deleteRegisteredWorkspace(workspaceId: string): Promise<void> {
    if (deletingWorkspaceId) return;
    deletingWorkspaceId = workspaceId;
    actionError = null;
    try {
      await deleteWorkspaceMutation.mutateAsync(workspaceId);
    } catch (error) {
      actionError = errorMessage(error);
    } finally {
      deletingWorkspaceId = null;
    }
  }

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  return {
    get rootId() {
      return rootId;
    },
    get view() {
      return view;
    },
    get activeWorkspaces() {
      return activeWorkspaces;
    },
    get unavailableWorkspaces() {
      return unavailableWorkspaces;
    },
    get workspaceRoots() {
      return workspaceRoots;
    },
    get listing() {
      return listing;
    },
    get selectedRoot() {
      return selectedRoot;
    },
    get currentWorkspace() {
      return currentWorkspace;
    },
    get visibleEntries() {
      return visibleEntries;
    },
    get hiddenEntryCount() {
      return hiddenEntryCount;
    },
    get breadcrumbs() {
      return breadcrumbs;
    },
    get parentPath() {
      return parentPath;
    },
    get browserLoadError() {
      return browserLoadError;
    },
    get workspacesLoading() {
      return workspacesLoading;
    },
    get actionsDisabled() {
      return actionsDisabled;
    },
    get actionError() {
      return actionError;
    },
    get registeringPath() {
      return registeringPath;
    },
    get showHiddenDirectories() {
      return showHiddenDirectories;
    },
    get renamingWorkspace() {
      return renamingWorkspace;
    },
    get renamingWorkspaceName() {
      return renamingWorkspaceName;
    },
    get renameWorkspaceDialogOpen() {
      return renameWorkspaceDialogOpen;
    },
    get renameError() {
      return renameError;
    },
    get savingRename() {
      return savingRename;
    },
    set actionError(value: string | null) {
      actionError = value;
    },
    set showHiddenDirectories(value: boolean) {
      showHiddenDirectories = value;
    },
    set renamingWorkspaceName(value: string) {
      renamingWorkspaceName = value;
    },
    set renameWorkspaceDialogOpen(value: boolean) {
      renameWorkspaceDialogOpen = value;
    },
    rootsQuery,
    entriesQuery,
    refreshAll,
    retryDirectory,
    openRoot,
    openPath,
    navigate,
    workspaceLocation,
    workspaceAvailabilityLabel,
    workspaceForEntry,
    workspaceName,
    togglePath,
    startRenamingWorkspace,
    cancelRenamingWorkspace,
    confirmRenameWorkspace,
    deleteRegisteredWorkspace,
  };
}
