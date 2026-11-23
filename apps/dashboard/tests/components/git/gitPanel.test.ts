import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../../deviceFixture";
import type { GitChangesView, GitDiffView } from "../../../src/api/types";
import type { GitPanel } from "../../../src/components/chat/git/gitPanelContext.svelte";
import GitPanelHarness from "./GitPanelHarness.svelte";

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

beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === "(min-width: 1024px)",
    addEventListener() {},
    removeEventListener() {},
  }));
  vi.spyOn(testDevice.api, "getWorkspaceGitChanges").mockResolvedValue(changes);
  vi.spyOn(testDevice.api, "getWorkspaceGitDiff").mockResolvedValue(diff);
});

afterEach(() => {
  cleanup();
  testDevice.queryClient.clear();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

test("does not read changes while the panel is closed", async () => {
  render(GitPanelHarness);
  await Promise.resolve();
  expect(testDevice.api.getWorkspaceGitChanges).not.toHaveBeenCalled();
});

test("lists changes by group in panel order with line counts", async () => {
  render(GitPanelHarness);
  await fireEvent.click(screen.getByRole("button", { name: "toggle" }));

  const panel = await screen.findByRole("complementary", { name: "Git changes" });
  await within(panel).findByText("lib.rs");
  const headings = within(panel)
    .getAllByRole("button", { expanded: true })
    .map((element) => element.textContent?.replace(/\s+/g, " ").trim());
  expect(headings).toEqual(["! Conflicted 1", "+ Staged 2", "~ Unstaged 1", "? Untracked 1"]);

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
  expect(localStorage.getItem("pontia.gitPanelOpen")).toBe("true");
});

test("opens a file diff in the panel and returns to the list", async () => {
  render(GitPanelHarness);
  await fireEvent.click(screen.getByRole("button", { name: "toggle" }));
  const panel = await screen.findByRole("complementary", { name: "Git changes" });

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

  await fireEvent.keyDown(panel, { key: "Escape" });
  await waitFor(() =>
    expect(within(panel).getByRole("button", { name: "Staged: src/lib.rs" })).toBe(
      document.activeElement,
    ),
  );
  await fireEvent.keyDown(panel, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
});

test("opens a diff by path from outside the panel", async () => {
  let open: ((file: { path: string; group: "staged" }) => void) | undefined;
  render(GitPanelHarness, {
    onpanel: (panel) => {
      open = (file) => panel.openFile(file);
    },
  });

  open!({ path: "src/lib.rs", group: "staged" });

  const panel = await screen.findByRole("complementary", { name: "Git changes" });
  await waitFor(() =>
    expect(panel.querySelector("[data-git-diff]")?.textContent).toContain("fn new() {}"),
  );
});

test("shows a message for a Workspace outside a Git repository", async () => {
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({
    repo_root: null,
    files: [],
    total: 0,
    truncated: false,
  });
  render(GitPanelHarness);
  await fireEvent.click(screen.getByRole("button", { name: "toggle" }));
  expect(await screen.findByText("Not a Git repository")).toBeTruthy();
});

test("keeps the open diff when the same Workspace attaches again", async () => {
  let panel: GitPanel | undefined;
  render(GitPanelHarness, { onpanel: (value: GitPanel) => (panel = value) });
  panel!.openFile({ path: "src/lib.rs", group: "staged" });

  // A session switch detaches the page while it loads, then attaches the same Workspace.
  const detach = panel!.attach({ device: testDevice, workspaceId: "workspace-1" });
  detach();
  panel!.attach({ device: testDevice, workspaceId: "workspace-1" });
  expect(panel!.file).toEqual({ path: "src/lib.rs", group: "staged" });

  panel!.attach({ device: testDevice, workspaceId: "workspace-2" });
  expect(panel!.file).toBeNull();
});
