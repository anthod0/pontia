import "../shallowRouting";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type {
  GitBranchesView,
  GitChangesView,
  GitCommitSummary,
  GitCommitView,
  GitDiffView,
  WorkspaceView,
} from "../../src/api/types";
import type { SessionPanel } from "../../src/components/chat/sessionPanel/sessionPanelContext.svelte";
import SessionPanelHarness from "./SessionPanelHarness.svelte";

const changes: GitChangesView = {
  repo_root: "/repo",
  total: 5,
  truncated: false,
  files: [
    {
      path: "src/conflict.ts",
      group: "conflicted",
      status: "U",
      additions: null,
      deletions: null,
      binary: false,
    },
    { path: "src/lib.rs", group: "staged", status: "M", additions: 3, deletions: 1, binary: false },
    {
      path: "docs/new.md",
      orig_path: "docs/old.md",
      group: "staged",
      status: "R",
      additions: 0,
      deletions: 0,
      binary: false,
    },
    {
      path: "logo.png",
      group: "unstaged",
      status: "M",
      additions: null,
      deletions: null,
      binary: true,
    },
    {
      path: "scratch/",
      group: "untracked",
      status: "?",
      additions: null,
      deletions: null,
      binary: false,
    },
  ],
};

const sha = (n: number) => String(n).repeat(40);

const branches: GitBranchesView = {
  head: "refs/heads/main",
  branches: [
    { name: "main", ref: "refs/heads/main", remote: false, current: true },
    { name: "origin/feature", ref: "refs/remotes/origin/feature", remote: true, current: false },
  ],
};

const summary = (n: number, subject: string): GitCommitSummary => ({
  sha: sha(n),
  parents: [sha(n + 1)],
  author_name: "Ada",
  authored_at: "2026-11-20T00:00:00Z",
  subject,
});

const commit: GitCommitView = {
  ...summary(9, "feature commit"),
  author_email: "ada@example.com",
  committer_name: "Ada",
  committed_at: "2026-11-20T00:00:00Z",
  body: "Explains the change.",
  total: 2,
  truncated: false,
  files: [
    { path: "src/lib.rs", status: "M", additions: 3, deletions: 1, binary: false },
    {
      path: "docs/new.md",
      orig_path: "docs/old.md",
      status: "R",
      additions: 0,
      deletions: 0,
      binary: false,
    },
  ],
};

const diff: GitDiffView = {
  path: "src/lib.rs",
  group: "staged",
  binary: false,
  truncated: false,
  hunks: [
    {
      header: "@@ -1,2 +1,2 @@",
      lines: [
        { kind: "context", old: 1, new: 1, text: "fn a() {}" },
        { kind: "delete", old: 2, new: null, text: "fn old() {}" },
        { kind: "add", old: null, new: 2, text: "fn new() {}" },
      ],
    },
  ],
};

const workspace: WorkspaceView = {
  availability: { status: "available", root_id: "root-1", path: "pontia" },
  workspace_id: "workspace-1",
  name: "pontia",
  canonical_path: "/repo/pontia",
  display_path: "~/repo/pontia",
  state: "active",
  metadata: {},
  created_at: "2026-11-20T00:00:00Z",
  updated_at: "2026-11-20T00:00:00Z",
  last_used_at: null,
};

let wide = true;

beforeEach(() => {
  localStorage.clear();
  wide = true;
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === (wide ? "(min-width: 1024px)" : "(max-width: 1023px)"),
    addEventListener() {},
    removeEventListener() {},
  }));
  vi.spyOn(testDevice.api, "listWorkspaces").mockResolvedValue([workspace]);
  vi.spyOn(testDevice.api, "getWorkspaceGitChanges").mockResolvedValue(changes);
  vi.spyOn(testDevice.api, "getWorkspaceGitDiff").mockResolvedValue(diff);
  vi.spyOn(testDevice.api, "getWorkspaceGitBranches").mockResolvedValue(branches);
  vi.spyOn(testDevice.api, "getWorkspaceGitCommits").mockImplementation(async (_, ref, skip) =>
    ref === "refs/heads/main"
      ? { commits: [summary(1, "add history view"), summary(2, "fix list")], has_more: true }
      : ref === sha(1) && skip === 2
        ? { commits: [summary(3, "older work")], has_more: false }
        : { commits: [summary(9, "feature commit")], has_more: false },
  );
  vi.spyOn(testDevice.api, "getWorkspaceGitCommit").mockResolvedValue(commit);
  vi.spyOn(testDevice.api, "getWorkspaceGitCommitDiff").mockResolvedValue({
    sha: sha(9),
    path: "src/lib.rs",
    binary: false,
    truncated: false,
    hunks: diff.hunks,
  });
  vi.spyOn(testDevice.fileApi, "pathFileContent").mockImplementation(
    async (_session, path) => new Response(`# ${path.slice(path.lastIndexOf("/") + 1, -3)}\n`),
  );
});

afterEach(() => {
  cleanup();
  testDevice.queryClient.clear();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const sidebarButton = () => screen.getByRole("button", { name: "Sidebar" });
const sidePanel = () => screen.getByRole("complementary", { name: "Sidebar" });

async function openTab(name: "Files" | "Git") {
  await fireEvent.click(sidebarButton());
  const panel = sidePanel();
  await fireEvent.click(within(panel).getByRole("tab", { name }));
  return panel;
}

test("the sidebar button opens the panel on Files and shows no change count", async () => {
  render(SessionPanelHarness);
  const button = sidebarButton();
  expect(button).toHaveAttribute("aria-pressed", "false");
  expect(button.textContent?.trim()).toBe("");
  expect(screen.queryByRole("complementary")).toBeNull();

  button.focus();
  await fireEvent.click(button);
  const panel = sidePanel();
  expect(button).toHaveAttribute("aria-pressed", "true");
  expect(within(panel).getByRole("tab", { name: "Files" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(within(panel).getByText("No open file")).toBeVisible();
  // Changes are read only once the Git tab shows.
  expect(testDevice.api.getWorkspaceGitChanges).not.toHaveBeenCalled();

  await fireEvent.click(within(panel).getByRole("button", { name: "Close" }));
  expect(screen.queryByRole("complementary")).toBeNull();
  expect(button).toHaveFocus();
});

test("returns to the latest localhost page after switching tabs and reopening the sidebar", async () => {
  let panel!: SessionPanel;
  render(SessionPanelHarness, { onpanel: (value) => (panel = value) });
  const url = "http://localhost:3000/report?q=1#total";
  panel.openPreview({ port: 3000, redirect: "/report?q=1#total", rawHref: url });
  expect(await screen.findByTitle("Open preview")).toHaveAttribute("src", url);

  const address = screen.getByRole("textbox", { name: "Preview address" });
  await fireEvent.input(address, { target: { value: "/latest" } });
  await fireEvent.submit(address.closest("form")!);
  await fireEvent.click(screen.getByRole("tab", { name: "Files" }));
  await fireEvent.click(sidebarButton());
  await fireEvent.click(sidebarButton());
  await fireEvent.click(screen.getByRole("tab", { name: "Preview" }));
  expect(await screen.findByTitle("Open preview")).toHaveAttribute(
    "src",
    "http://localhost:3000/latest",
  );
});

test("lists changes by group in panel order with line counts", async () => {
  render(SessionPanelHarness);
  const panel = await openTab("Git");

  await within(panel).findByText("lib.rs");
  const headings = within(panel)
    .getAllByRole("button", { expanded: true })
    .map((element) => element.textContent?.replace(/\s+/g, " ").trim());
  expect(headings).toEqual(["Conflicted 1", "Staged 2", "Unstaged 1", "Untracked 1"]);

  const lib = within(panel).getByRole("button", { name: "Staged: src/lib.rs" });
  expect(lib.textContent).toContain("+3");
  expect(lib.textContent).toContain("−1");
  expect(within(panel).getByRole("button", { name: "Staged: docs/new.md" }).title).toBe(
    "docs/old.md → docs/new.md",
  );
  expect(within(panel).getByRole("button", { name: "Unstaged: logo.png" }).textContent).toContain(
    "Binary",
  );
  expect(within(panel).getByRole("button", { name: "Untracked: scratch/" })).toBeDisabled();
  expect(await within(panel).findByText("pontia")).toHaveAttribute("title", "/repo/pontia");
  expect(within(panel).getByTitle("5 changed files")).toHaveTextContent("5");
});

test("opens a file diff in the Git tab and returns to the list", async () => {
  render(SessionPanelHarness);
  const panel = await openTab("Git");

  await fireEvent.click(await within(panel).findByRole("button", { name: "Staged: src/lib.rs" }));

  await waitFor(() =>
    expect(panel.querySelector("[data-git-diff]")?.textContent).toContain("fn new() {}"),
  );
  expect(testDevice.api.getWorkspaceGitDiff).toHaveBeenCalledWith(
    "workspace-1",
    { path: "src/lib.rs", orig_path: undefined, group: "staged" },
    expect.anything(),
  );
  expect(within(panel).getByText("@@ -1,2 +1,2 @@")).toBeTruthy();

  // Esc leaves the diff, and on the list it leaves the side panel open.
  await fireEvent.keyDown(panel, { key: "Escape" });
  await waitFor(() =>
    expect(within(panel).getByRole("button", { name: "Staged: src/lib.rs" })).toBe(
      document.activeElement,
    ),
  );
  await fireEvent.keyDown(panel, { key: "Escape" });
  expect(sidePanel()).toBeInTheDocument();
});

test("local file links open in the Files tab, which lists them again after a reload", async () => {
  const first = render(SessionPanelHarness);
  await openTab("Git");

  await fireEvent.click(screen.getByRole("button", { name: "Preview notes.md" }));
  const panel = sidePanel();
  expect(within(panel).getByRole("tab", { name: "Files" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await within(panel).findByRole("heading", { name: "notes" });
  await fireEvent.click(within(panel).getByRole("button", { name: "Close file" }));

  first.unmount();
  const second = render(SessionPanelHarness);
  const list = await within(sidePanel()).findByRole("region", { name: "Recently opened" });
  await fireEvent.click(within(list).getByRole("button", { name: "Preview notes.md" }));
  await within(sidePanel()).findByRole("heading", { name: "notes" });

  // Another Session on the same panel closes the file and lists only its own files.
  await second.rerender({ sessionId: "session-2" });
  expect(await within(sidePanel()).findByText("No open file")).toBeVisible();
  await second.rerender({ sessionId: "session-1" });
  expect(await within(sidePanel()).findByRole("region", { name: "Recently opened" })).toBeVisible();
});

test("resizes the panel with the keyboard and remembers the width", async () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 400,
  } as DOMRect);
  vi.stubGlobal("innerWidth", 1440);
  render(SessionPanelHarness);
  await fireEvent.click(sidebarButton());
  const handle = within(sidePanel()).getByRole("separator", { name: "Resize preview" });
  await fireEvent.keyDown(handle, { key: "ArrowLeft" });
  expect(localStorage.getItem("pontia.filePreviewWidth")).toBe("432");
  expect(handle).toHaveAttribute("aria-valuenow", "432");
  await fireEvent.dblClick(handle);
  expect(localStorage.getItem("pontia.filePreviewWidth")).toBeNull();
});

test("on wide screens the panel stays open on the tab last used", async () => {
  const first = render(SessionPanelHarness);
  await openTab("Git");
  first.unmount();
  expect(localStorage.getItem("pontia.gitPanelOpen")).toBe("true");
  expect(localStorage.getItem("pontia.sessionPanelTab")).toBe("git");

  render(SessionPanelHarness);
  expect(within(sidePanel()).getByRole("tab", { name: "Git" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});

test("phone history traverses the panel without leaving the session or changing desktop preferences", async () => {
  localStorage.setItem("pontia.gitPanelOpen", "true");
  wide = false;
  render(SessionPanelHarness);
  expect(screen.queryByRole("dialog")).toBeNull();
  sidebarButton().focus();
  await fireEvent.click(sidebarButton());
  const dialog = await screen.findByRole("dialog", { name: "Sidebar" });
  const url = location.href;
  history.back();
  await waitFor(() => expect(dialog).not.toBeInTheDocument());
  expect(location.href).toBe(url);
  await waitFor(() => expect(sidebarButton()).toHaveFocus());
  history.forward();
  const restored = await screen.findByRole("dialog", { name: "Sidebar" });
  await fireEvent.click(within(restored).getByRole("button", { name: "Close" }));
  await waitFor(() => expect(restored).not.toBeInTheDocument());
  expect(location.href).toBe(url);
  expect(localStorage.getItem("pontia.gitPanelOpen")).toBe("true");
});

test("toggling Git opens the Git tab, then closes the panel", async () => {
  let panel: SessionPanel | undefined;
  render(SessionPanelHarness, { onpanel: (value: SessionPanel) => (panel = value) });
  panel!.toggleGit();
  const aside = await screen.findByRole("complementary", { name: "Sidebar" });
  expect(within(aside).getByRole("tab", { name: "Git" })).toHaveAttribute("aria-selected", "true");
  await waitFor(() => expect(aside).toBe(document.activeElement));
  panel!.toggleGit();
  await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
});

test("before the first commit, still offers the other branches", async () => {
  vi.mocked(testDevice.api.getWorkspaceGitBranches).mockResolvedValue({
    head: null,
    branches: [{ name: "main", ref: "refs/heads/main", remote: false, current: false }],
  });
  render(SessionPanelHarness);
  const panel = await openTab("Git");
  await fireEvent.click(await within(panel).findByRole("tab", { name: "History" }));
  expect(await within(panel).findByText("No commits yet")).toBeVisible();
  expect(testDevice.api.getWorkspaceGitCommits).not.toHaveBeenCalled();

  const picker = within(panel).getByRole("combobox", { name: "Branch to show: No branch" });
  await fireEvent.keyDown(picker, { key: "ArrowDown" });
  await screen.findByRole("option", { name: /main/ });
  await fireEvent.keyDown(picker, { key: "Enter" });
  await within(panel).findByRole("button", { name: /add history view/ });
});

test("shows a message for a Workspace outside a Git repository", async () => {
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({
    repo_root: null,
    files: [],
    total: 0,
    truncated: false,
  });
  render(SessionPanelHarness);
  await openTab("Git");
  expect(await screen.findByText("Not a Git repository")).toBeTruthy();
});

test("shows a message for a Session without a Workspace", async () => {
  render(SessionPanelHarness, { workspaceId: null });
  await openTab("Git");
  expect(await screen.findByText("No workspace")).toBeTruthy();
  expect(testDevice.api.getWorkspaceGitChanges).not.toHaveBeenCalled();
});

test("keeps what the Git tab opened when the same Workspace attaches again", () => {
  let panel: SessionPanel | undefined;
  render(SessionPanelHarness, { onpanel: (value: SessionPanel) => (panel = value) });
  panel!.git.diff = { path: "src/lib.rs", group: "staged" };
  panel!.git.view = "history";
  panel!.git.chooseRef("refs/heads/feature");
  panel!.git.openCommit(sha(1));

  // A session switch detaches the page while it loads, then attaches the same Workspace.
  const source = { device: testDevice, sessionId: "session-2", workspaceId: "workspace-1" };
  panel!.attach(source)();
  panel!.attach(source);
  expect(panel!.git.diff).toEqual({ path: "src/lib.rs", group: "staged" });
  expect(panel!.git.commit).toBe(sha(1));

  // Another Workspace keeps the chosen list but forgets the branch, commit and diff.
  panel!.attach({ ...source, workspaceId: "workspace-2" });
  expect(panel!.git.diff).toBeNull();
  expect(panel!.git.ref).toBeNull();
  expect(panel!.git.commit).toBeNull();
  expect(panel!.git.view).toBe("history");
});

test("opens history when the changes request fails", async () => {
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockRejectedValue(
    new Error("Cannot read changes"),
  );
  render(SessionPanelHarness);
  const panel = await openTab("Git");
  await within(panel).findByRole("alert");
  await fireEvent.click(within(panel).getByRole("tab", { name: "History" }));
  await within(panel).findByRole("button", { name: /add history view/ });
});

test("reads a chosen branch's history and a commit's file diffs without checking out", async () => {
  render(SessionPanelHarness);
  const panel = await openTab("Git");
  // History is read only once it shows.
  await within(panel).findByRole("tab", { name: "History" });
  expect(testDevice.api.getWorkspaceGitBranches).not.toHaveBeenCalled();
  await fireEvent.click(within(panel).getByRole("tab", { name: "History" }));

  // The checked-out branch shows first.
  await within(panel).findByRole("button", { name: /add history view/ });
  expect(testDevice.api.getWorkspaceGitCommits).toHaveBeenCalledWith(
    "workspace-1",
    "refs/heads/main",
    0,
    expect.anything(),
  );
  const picker = within(panel).getByRole("combobox", { name: "Branch to show: main" });
  expect(picker).toHaveTextContent("Current");

  // More history loads on request, from the tip the first page read.
  await fireEvent.click(within(panel).getByRole("button", { name: "Load more" }));
  await within(panel).findByRole("button", { name: /older work/ });
  expect(testDevice.api.getWorkspaceGitCommits).toHaveBeenLastCalledWith(
    "workspace-1",
    sha(1),
    2,
    expect.anything(),
  );

  // Choosing another branch reads its history; there is no checkout request to make.
  await fireEvent.keyDown(picker, { key: "ArrowDown" });
  await screen.findByRole("option", { name: /origin\/feature/ });
  await fireEvent.keyDown(picker, { key: "End" });
  await fireEvent.keyDown(picker, { key: "Enter" });
  await within(panel).findByRole("button", { name: /feature commit/ });
  expect(testDevice.api.getWorkspaceGitCommits).toHaveBeenLastCalledWith(
    "workspace-1",
    "refs/remotes/origin/feature",
    0,
    expect.anything(),
  );
  expect(
    within(panel).getByRole("combobox", { name: "Branch to show: origin/feature" }),
  ).toBeVisible();

  await fireEvent.click(within(panel).getByRole("button", { name: /feature commit/ }));
  await within(panel).findByText("Explains the change.");
  expect(testDevice.api.getWorkspaceGitCommit).toHaveBeenCalledWith(
    "workspace-1",
    sha(9),
    expect.anything(),
  );
  expect(within(panel).getByRole("button", { name: "docs/new.md" }).title).toBe(
    "docs/old.md → docs/new.md",
  );

  await fireEvent.click(within(panel).getByRole("button", { name: "src/lib.rs" }));
  await waitFor(() =>
    expect(panel.querySelector("[data-git-diff]")?.textContent).toContain("fn new() {}"),
  );
  expect(testDevice.api.getWorkspaceGitCommitDiff).toHaveBeenCalledWith(
    "workspace-1",
    sha(9),
    { path: "src/lib.rs", orig_path: undefined },
    expect.anything(),
  );
  expect(within(panel).getByText("+3")).toBeVisible();

  // Esc steps back to the commit, then to the history, with focus on the row it came from.
  await fireEvent.keyDown(panel, { key: "Escape" });
  await waitFor(() =>
    expect(within(panel).getByRole("button", { name: "src/lib.rs" })).toBe(document.activeElement),
  );
  await fireEvent.keyDown(panel, { key: "Escape" });
  await waitFor(() =>
    expect(within(panel).getByRole("button", { name: /feature commit/ })).toBe(
      document.activeElement,
    ),
  );
  expect(
    within(panel).getByRole("combobox", { name: "Branch to show: origin/feature" }),
  ).toBeVisible();
});
