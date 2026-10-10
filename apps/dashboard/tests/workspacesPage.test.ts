import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import WorkspacesPage from "../src/pages/WorkspacesPage.svelte";
import type {
  WorkspaceDirectoryListingView,
  WorkspaceRootView,
  WorkspaceView,
} from "../src/api/types";

const mocks = vi.hoisted(() => {
  function writableStore<T>(initial: T) {
    let value = initial;
    const subscribers = new Set<(value: T) => void>();
    return {
      subscribe(run: (value: T) => void) {
        subscribers.add(run);
        run(value);
        return () => subscribers.delete(run);
      },
      set(next: T) {
        value = next;
        for (const run of subscribers) run(value);
      },
      update(updater: (value: T) => T) {
        value = updater(value);
        for (const run of subscribers) run(value);
      },
    };
  }

  const workspaces = writableStore<WorkspaceView[]>([]);
  const workspacesLoading = writableStore(false);
  const workspacesError = writableStore<string | null>(null);
  const workspaceGitStatuses = writableStore({});
  const workspaceGitStatusErrors = writableStore({});

  return {
    workspaces,
    workspacesLoading,
    workspacesError,
    workspaceGitStatuses,
    workspaceGitStatusErrors,
    roots: [] as WorkspaceRootView[],
    listing: null as WorkspaceDirectoryListingView | null,
    loadWorkspaces: vi.fn(async () => undefined),
    listWorkspaceRoots: vi.fn(async () => mocks.roots),
    listWorkspaceRootEntries: vi.fn(async (_rootId: string, path = "") => {
      if (path === "missing-workspace") throw new Error("directory not found");
      return mocks.listing;
    }),
    loadWorkspaceGitStatus: vi.fn(async () => undefined),
    refreshWorkspaceGitStatus: vi.fn(async () => undefined),
    registerWorkspace: vi.fn(async () => undefined),
    renameWorkspace: vi.fn(async () => undefined),
    deleteWorkspace: vi.fn(async () => undefined),
  };
});

vi.mock("../src/stores/workspaces", () => ({
  workspaces: mocks.workspaces,
  workspacesLoading: mocks.workspacesLoading,
  workspacesError: mocks.workspacesError,
  workspaceGitStatuses: mocks.workspaceGitStatuses,
  workspaceGitStatusErrors: mocks.workspaceGitStatusErrors,
  loadWorkspaces: mocks.loadWorkspaces,
  loadWorkspaceGitStatus: mocks.loadWorkspaceGitStatus,
  refreshWorkspaceGitStatus: mocks.refreshWorkspaceGitStatus,
  registerWorkspace: mocks.registerWorkspace,
  renameWorkspace: mocks.renameWorkspace,
  deleteWorkspace: mocks.deleteWorkspace,
}));

vi.mock("../src/api/client", () => ({
  listWorkspaceRoots: mocks.listWorkspaceRoots,
  listWorkspaceRootEntries: mocks.listWorkspaceRootEntries,
}));

const workspace = (overrides: Partial<WorkspaceView> = {}): WorkspaceView => ({
  workspace_id: "workspace-1",
  name: "pontia",
  canonical_path: "/repo/pontia",
  display_path: "/repo/pontia",
  state: "active",
  metadata: {},
  created_at: "2026-05-14T00:00:00Z",
  updated_at: "2026-05-14T00:00:00Z",
  last_used_at: null,
  ...overrides,
});

beforeEach(() => {
  mocks.roots = [
    { root_id: "root-1", label: "Projects", canonical_path: "/repo", state: "active" },
  ];
  mocks.listing = {
    root_id: "root-1",
    path: "",
    canonical_path: "/repo",
    parent_path: null,
    entries: [
      { name: "pontia", path: "pontia", kind: "directory", is_workspace: true },
      { name: "sandbox", path: "sandbox", kind: "directory", is_workspace: false },
      { name: ".scratch", path: ".scratch", kind: "directory", is_workspace: false },
    ],
    warnings: [],
  };
  mocks.workspaces.set([workspace()]);
  mocks.workspacesLoading.set(false);
  mocks.workspacesError.set(null);
  mocks.workspaceGitStatuses.set({});
  mocks.workspaceGitStatusErrors.set({});
  vi.clearAllMocks();
});

test("lists active workspaces across roots in a dismissible dialog", async () => {
  const user = userEvent.setup();
  mocks.workspaces.set([
    workspace(),
    workspace({
      workspace_id: "other",
      name: "Other project",
      canonical_path: "/elsewhere/project",
    }),
    workspace({ workspace_id: "inactive", name: "Inactive project", state: "deleted" }),
  ]);
  render(WorkspacesPage);

  await user.click(screen.getByRole("button", { name: "Active workspaces" }));
  const dialog = screen.getByRole("dialog", { name: "Active workspaces" });
  expect(within(dialog).getAllByRole("listitem")).toHaveLength(2);
  expect(within(dialog).getByText("pontia")).toBeInTheDocument();
  expect(within(dialog).getByText("/repo/pontia")).toBeInTheDocument();
  expect(within(dialog).getByText("Other project")).toBeInTheDocument();
  expect(within(dialog).getByText("/elsewhere/project")).toBeInTheDocument();
  expect(within(dialog).queryByText("Inactive project")).not.toBeInTheDocument();

  await user.click(within(dialog).getAllByRole("button", { name: "Close", exact: true })[0]);
  expect(screen.queryByRole("dialog", { name: "Active workspaces" })).not.toBeInTheDocument();
});

test("shows an empty active workspace list", async () => {
  const user = userEvent.setup();
  mocks.workspaces.set([]);
  render(WorkspacesPage);

  await user.click(screen.getByRole("button", { name: "Active workspaces" }));
  expect(
    within(screen.getByRole("dialog", { name: "Active workspaces" })).getByText(
      "No active workspaces.",
    ),
  ).toBeInTheDocument();
});

test("opens directories through the folder-name button", async () => {
  const user = userEvent.setup();
  render(WorkspacesPage);

  await user.click(await screen.findByRole("button", { name: "Enter directory sandbox" }));
  expect(mocks.listWorkspaceRootEntries).toHaveBeenLastCalledWith(
    "root-1",
    "sandbox",
    expect.objectContaining({ signal: expect.any(AbortSignal) }),
  );
});

test("hides dot-directories by default and allows showing them", async () => {
  const user = userEvent.setup();
  render(WorkspacesPage);

  await screen.findByRole("table");
  const showHiddenButton = screen.getByRole("button", { name: "Show hidden" });
  expect(showHiddenButton).toHaveAttribute("aria-pressed", "false");
  expect(screen.queryByRole("button", { name: "Open directory .scratch" })).not.toBeInTheDocument();

  await user.click(showHiddenButton);

  expect(showHiddenButton).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("button", { name: "Open directory .scratch" })).toBeInTheDocument();
});

test("shows outside-root active workspace banner and revokes workspaces from the dialog", async () => {
  const user = userEvent.setup();
  mocks.roots = [
    { root_id: "root-1", label: "Projects", canonical_path: "/repo/project", state: "available" },
  ];
  mocks.workspaces.set([
    workspace({
      workspace_id: "inside",
      name: "inside",
      canonical_path: "/repo/project/app",
      display_path: "/repo/project/app",
    }),
    workspace({
      workspace_id: "outside",
      name: "outside",
      canonical_path: "/repo/project-other/app",
      display_path: "/repo/project-other/app",
    }),
  ]);

  render(WorkspacesPage);

  expect(await screen.findByText("1 unavailable active workspace")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Review" }));

  const dialog = screen.getByRole("dialog", { name: "Unavailable active workspaces" });
  expect(within(dialog).getByText("/repo/project-other/app")).toBeInTheDocument();
  expect(within(dialog).queryByText("/repo/project/app")).not.toBeInTheDocument();

  await user.click(within(dialog).getByRole("button", { name: "Revoke outside" }));

  expect(mocks.deleteWorkspace).toHaveBeenCalledWith("outside");
});

test("shows unavailable active workspaces when the workspace directory is missing under a configured root", async () => {
  const user = userEvent.setup();
  mocks.roots = [
    { root_id: "root-1", label: "Projects", canonical_path: "/repo/project", state: "available" },
  ];
  mocks.workspaces.set([
    workspace({
      workspace_id: "missing",
      name: "missing-workspace",
      canonical_path: "/repo/project/missing-workspace",
      display_path: "/repo/project/missing-workspace",
    }),
  ]);

  render(WorkspacesPage);

  expect(await screen.findByText("1 unavailable active workspace")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Review" }));

  const dialog = screen.getByRole("dialog", { name: "Unavailable active workspaces" });
  expect(within(dialog).getByText("/repo/project/missing-workspace")).toBeInTheDocument();
  expect(within(dialog).getByText("Missing directory")).toBeInTheDocument();

  await user.click(within(dialog).getByRole("button", { name: "Revoke missing-workspace" }));

  expect(mocks.deleteWorkspace).toHaveBeenCalledWith("missing");
});

test("toggles workspace active state directly and keeps rename dialog for editing names", async () => {
  const user = userEvent.setup();
  const confirmSpy = vi.spyOn(window, "confirm");
  render(WorkspacesPage);

  const activateButton = await screen.findByRole("button", { name: "Activate sandbox" });
  await user.click(activateButton);

  expect(mocks.registerWorkspace).toHaveBeenCalledWith({
    root_id: "root-1",
    path: "sandbox",
    name: "sandbox",
  });
  await user.click(screen.getByRole("button", { name: "Deactivate pontia" }));

  expect(confirmSpy).not.toHaveBeenCalled();
  expect(mocks.deleteWorkspace).toHaveBeenCalledWith("workspace-1");

  await user.click(screen.getByRole("button", { name: "Rename pontia" }));

  expect(screen.getByRole("dialog", { name: "Confirm workspace rename" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Confirm workspace rename" })).toBeInTheDocument();
  expect(screen.getByLabelText("Display name")).toHaveValue("pontia");

  confirmSpy.mockRestore();
});

test("aborts initial settings workspace requests when the page unmounts", async () => {
  mocks.listWorkspaceRoots.mockImplementationOnce(() => new Promise(() => {}));
  const { unmount } = render(WorkspacesPage);

  await vi.waitFor(() => expect(mocks.loadWorkspaces).toHaveBeenCalled());
  const workspaceOptions = mocks.loadWorkspaces.mock.calls[0][0] as
    | { signal?: AbortSignal }
    | undefined;
  await vi.waitFor(() => expect(mocks.listWorkspaceRoots).toHaveBeenCalled());
  const rootsOptions = mocks.listWorkspaceRoots.mock.calls[0][0] as
    | { signal?: AbortSignal }
    | undefined;

  expect(workspaceOptions?.signal).toBeInstanceOf(AbortSignal);
  expect(rootsOptions?.signal).toBeInstanceOf(AbortSignal);
  expect(workspaceOptions?.signal?.aborted).toBe(false);
  expect(rootsOptions?.signal?.aborted).toBe(false);

  unmount();

  expect(workspaceOptions?.signal?.aborted).toBe(true);
  expect(rootsOptions?.signal?.aborted).toBe(true);
});
