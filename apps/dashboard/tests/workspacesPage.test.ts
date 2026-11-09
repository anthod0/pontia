import { testDevice } from "./deviceFixture";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import WorkspacesPage from "../src/pages/WorkspacesPage.svelte";
const { queryClient } = testDevice;
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
      get() {
        return value;
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
      if (!mocks.listing || path === mocks.listing.path) return mocks.listing;
      return {
        ...mocks.listing,
        path,
        canonical_path: `${mocks.listing.canonical_path}/${path}`,
        parent_path: "",
        entries: [],
      };
    }),
    loadWorkspaceGitStatus: vi.fn(async () => undefined),
    refreshWorkspaceGitStatus: vi.fn(async () => undefined),
    registerWorkspace: vi.fn(async () => undefined),
    renameWorkspace: vi.fn(async () => undefined),
    deleteWorkspace: vi.fn(async () => undefined),
  };
});

vi.mock("../src/api/client", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    listWorkspaces: async (options: unknown) => {
      await mocks.loadWorkspaces(options);
      const error = mocks.workspacesError.get();
      if (error) throw new Error(error);
      return mocks.workspaces.get();
    },
    listWorkspaceRoots: mocks.listWorkspaceRoots,
    listWorkspaceRootEntries: mocks.listWorkspaceRootEntries,
    registerWorkspace: mocks.registerWorkspace,
    renameWorkspace: (workspaceId: string, input: unknown) =>
      mocks.renameWorkspace(workspaceId, input),
    deleteWorkspace: mocks.deleteWorkspace,
  };
  return {
    ...original,
    ...mocked,
    createApi: (device) => ({ ...original.createApi(device), ...mocked }),
  };
});

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
  queryClient.clear();
  mocks.roots = [
    { root_id: "root-1", label: "Projects", canonical_path: "/repo", state: "available" },
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
  mocks.registerWorkspace.mockResolvedValue(workspace());
  mocks.renameWorkspace.mockResolvedValue(workspace());
  mocks.deleteWorkspace.mockResolvedValue(workspace());
});

test("lists active workspaces across roots from the browser sidebar", async () => {
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

  await user.click(await screen.findByRole("button", { name: /^Active\b/ }));
  const list = screen.getByRole("list", { name: "Active workspaces" });
  expect(within(list).getAllByRole("listitem")).toHaveLength(2);
  expect(within(list).getByText("pontia")).toBeInTheDocument();
  expect(within(list).getByText("/repo/pontia")).toBeInTheDocument();
  expect(within(list).getByText("Other project")).toBeInTheDocument();
  expect(within(list).getByText("/elsewhere/project")).toBeInTheDocument();
  expect(within(list).queryByText("Inactive project")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Open workspace Other project" })).toBeDisabled();

  await user.click(screen.getByRole("button", { name: "Open workspace pontia" }));
  expect(mocks.listWorkspaceRootEntries).toHaveBeenLastCalledWith(
    "root-1",
    "pontia",
    expect.objectContaining({ signal: expect.any(AbortSignal) }),
  );
});

test("hides the Unavailable entry when every active workspace is reachable", async () => {
  render(WorkspacesPage);

  await screen.findByRole("list", { name: "Directories" });
  await vi.waitFor(() =>
    expect(mocks.listWorkspaceRootEntries).toHaveBeenCalledWith(
      "root-1",
      "pontia",
      expect.anything(),
    ),
  );
  expect(screen.queryByRole("button", { name: /^Unavailable/ })).not.toBeInTheDocument();
});

test("shows an empty active workspace list", async () => {
  const user = userEvent.setup();
  mocks.workspaces.set([]);
  render(WorkspacesPage);

  await user.click(await screen.findByRole("button", { name: /^Active\b/ }));
  expect(screen.getByText("No active workspaces")).toBeInTheDocument();
});

test("opens a directory with a single click", async () => {
  const user = userEvent.setup();
  render(WorkspacesPage);

  await user.click(await screen.findByRole("button", { name: "Open directory sandbox" }));
  expect(mocks.listWorkspaceRootEntries).toHaveBeenLastCalledWith(
    "root-1",
    "sandbox",
    expect.objectContaining({ signal: expect.any(AbortSignal) }),
  );
});

test("opens the parent directory from the toolbar", async () => {
  const user = userEvent.setup();
  render(WorkspacesPage);

  const parent = await screen.findByRole("button", { name: "Open parent directory" });
  expect(parent).toBeDisabled();

  await user.click(await screen.findByRole("button", { name: "Open directory sandbox" }));
  const path = screen.getByRole("navigation", { name: "Current path" });
  expect(await within(path).findByRole("button", { name: "sandbox" })).toHaveAttribute(
    "aria-current",
    "page",
  );

  await user.click(parent);
  expect(mocks.listWorkspaceRootEntries).toHaveBeenLastCalledWith(
    "root-1",
    "",
    expect.objectContaining({ signal: expect.any(AbortSignal) }),
  );
  await vi.waitFor(() => expect(parent).toBeDisabled());
  expect(within(path).queryByRole("button", { name: "sandbox" })).not.toBeInTheDocument();
});

test("hides dot-directories by default and allows showing them", async () => {
  const user = userEvent.setup();
  render(WorkspacesPage);

  await screen.findByRole("list", { name: "Directories" });
  const showHiddenButton = screen.getByRole("button", { name: "Show hidden" });
  expect(showHiddenButton).toHaveAttribute("aria-pressed", "false");
  expect(screen.queryByRole("button", { name: "Open directory .scratch" })).not.toBeInTheDocument();

  await user.click(showHiddenButton);

  expect(showHiddenButton).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("button", { name: "Open directory .scratch" })).toBeInTheDocument();
});

test("lists outside-root workspaces under Unavailable in the sidebar and revokes them", async () => {
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

  await user.click(await screen.findByRole("button", { name: /^Unavailable/ }));

  const list = screen.getByRole("list", { name: "Unavailable workspaces" });
  expect(within(list).getByText("/repo/project-other/app")).toBeInTheDocument();
  expect(within(list).queryByText("/repo/project/app")).not.toBeInTheDocument();

  await user.click(within(list).getByRole("button", { name: "Revoke outside" }));

  expect(mocks.deleteWorkspace).toHaveBeenCalledWith("outside");
});

test("lists workspaces with missing directories under Unavailable", async () => {
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

  await user.click(await screen.findByRole("button", { name: /^Unavailable/ }));

  const list = screen.getByRole("list", { name: "Unavailable workspaces" });
  expect(within(list).getByText("/repo/project/missing-workspace")).toBeInTheDocument();
  expect(within(list).getByText("· Missing directory")).toBeInTheDocument();

  await user.click(within(list).getByRole("button", { name: "Revoke missing-workspace" }));

  expect(mocks.deleteWorkspace).toHaveBeenCalledWith("missing");
});

test("activates a workspace directly and keeps rename dialog for editing names", async () => {
  const user = userEvent.setup();
  render(WorkspacesPage);

  const activateButton = await screen.findByRole("button", { name: "Activate sandbox" });
  await vi.waitFor(() => expect(activateButton).toBeEnabled());
  await user.click(activateButton);

  expect(mocks.registerWorkspace).toHaveBeenCalledWith({
    root_id: "root-1",
    path: "sandbox",
    name: "sandbox",
  });

  await user.click(await screen.findByRole("button", { name: "Rename pontia" }));

  expect(screen.getByRole("dialog", { name: "Confirm workspace rename" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Confirm workspace rename" })).toBeInTheDocument();
  expect(screen.getByLabelText("Display name")).toHaveValue("pontia");
});

test("shows a directory load error inside the browser and keeps navigation usable", async () => {
  const user = userEvent.setup();
  mocks.listWorkspaceRootEntries.mockImplementation(async (_rootId: string, path = "") => {
    if (path === "sandbox") throw new Error("Directory not found");
    return mocks.listing;
  });
  render(WorkspacesPage);

  await user.click(await screen.findByRole("button", { name: "Open directory sandbox" }));

  const alert = await screen.findByRole("alert");
  expect(within(alert).getByText("Cannot open this directory")).toBeInTheDocument();
  expect(within(alert).getByText("Directory not found")).toBeInTheDocument();
  expect(screen.queryByText("No root opened")).not.toBeInTheDocument();

  await user.click(within(alert).getByRole("button", { name: "Retry" }));
  expect(mocks.listWorkspaceRootEntries).toHaveBeenLastCalledWith(
    "root-1",
    "sandbox",
    expect.objectContaining({ signal: expect.any(AbortSignal) }),
  );

  await user.click(screen.getByRole("button", { name: "Open parent directory" }));
  expect(await screen.findByRole("list", { name: "Directories" })).toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("replaces the browser with a retryable error when roots fail to load", async () => {
  const user = userEvent.setup();
  mocks.listWorkspaceRoots.mockRejectedValueOnce(new Error("Pontia is offline"));
  render(WorkspacesPage);

  const alert = await screen.findByRole("alert");
  expect(within(alert).getByText("Workspace browser unavailable")).toBeInTheDocument();
  expect(within(alert).getByText("Pontia is offline")).toBeInTheDocument();

  await user.click(within(alert).getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("list", { name: "Directories" })).toBeInTheDocument();
});

test("shows a dismissible action error under the toolbar", async () => {
  const user = userEvent.setup();
  mocks.registerWorkspace.mockRejectedValueOnce(new Error("Permission denied"));
  render(WorkspacesPage);

  const activateButton = await screen.findByRole("button", { name: "Activate sandbox" });
  await vi.waitFor(() => expect(activateButton).toBeEnabled());
  await user.click(activateButton);

  const alert = await screen.findByRole("alert");
  expect(within(alert).getByText("Permission denied")).toBeInTheDocument();
  expect(screen.getByRole("list", { name: "Directories" })).toBeInTheDocument();

  await user.click(within(alert).getByRole("button", { name: "Dismiss error" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("shows rename errors inside the rename dialog", async () => {
  const user = userEvent.setup();
  mocks.renameWorkspace.mockRejectedValueOnce(new Error("Name already taken"));
  render(WorkspacesPage);

  await user.click(await screen.findByRole("button", { name: "Rename pontia" }));
  const dialog = screen.getByRole("dialog", { name: "Confirm workspace rename" });
  await user.click(within(dialog).getByRole("button", { name: "Rename workspace" }));

  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Name already taken");
});

test("aborts initial settings workspace requests when the page unmounts", async () => {
  mocks.loadWorkspaces.mockImplementationOnce(() => new Promise(() => {}));
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

  await vi.waitFor(() => expect(workspaceOptions?.signal?.aborted).toBe(true));
  await vi.waitFor(() => expect(rootsOptions?.signal?.aborted).toBe(true));
});
