import "../shallowRouting";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot, GitChangesView, WorkspaceView } from "../../src/api/types";
import BotChatPreviewHarness from "./BotChatPreviewHarness.svelte";

vi.mock("../../src/services/botChatStream", () => ({ openBotChatStream: () => () => {} }));

const bot: Bot = {
  bot_id: "a",
  name: "Bot",
  description: null,
  avatar_shape: "classic",
  avatar_color: "blue",
  client_type: "pi",
  client_key: "default",
  model: "test",
  reasoning_effort: null,
};

const workspace = (overrides: Partial<WorkspaceView>): WorkspaceView => ({
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
  ...overrides,
});

const changes: GitChangesView = {
  repo_root: "/repo/pontia",
  total: 1,
  truncated: false,
  files: [
    { path: "src/lib.rs", group: "staged", status: "M", additions: 3, deletions: 1, binary: false },
  ],
};

beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query === "(min-width: 1024px)",
    addEventListener() {},
    removeEventListener() {},
  }));
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot,
    entries: [],
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
  });
  vi.spyOn(testDevice.api, "listWorkspaces").mockResolvedValue([
    workspace({}),
    workspace({ workspace_id: "workspace-2", name: "docs", display_path: "~/repo/docs" }),
    workspace({
      workspace_id: "workspace-3",
      name: "gone",
      availability: { status: "unavailable", reason: "missing_directory" },
    }),
  ]);
  vi.spyOn(testDevice.api, "getWorkspaceGitBranches").mockResolvedValue({
    head: null,
    branches: [],
  });
  vi.spyOn(testDevice.api, "getWorkspaceGitChanges").mockResolvedValue(changes);
  vi.spyOn(testDevice.api, "getWorkspaceGitDiff").mockResolvedValue({
    path: "src/lib.rs",
    group: "staged",
    binary: false,
    truncated: false,
    hunks: [
      { header: "@@ -1 +1 @@", lines: [{ kind: "add", old: null, new: 1, text: "fn new() {}" }] },
    ],
  });
  vi.spyOn(testDevice.api, "refreshWorkspaceGitStatus").mockImplementation(async (id) => ({
    workspace_id: id,
    repo_root: "/repo/pontia",
    branch: "main",
    upstream: "origin/main",
    ahead: 1,
    behind: 0,
    staged_count: 1,
    unstaged_count: 0,
    untracked_count: 0,
    conflicted_count: 0,
    state: "ok",
    error: null,
    checked_at: "2026-11-20T00:00:00Z",
  }));
});

afterEach(() => {
  testDevice.queryClient.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

async function openGitTab() {
  await fireEvent.click(await screen.findByRole("button", { name: "Sidebar" }));
  await fireEvent.click(screen.getByRole("tab", { name: "Git" }));
  return screen.getByRole("complementary", { name: "Sidebar" });
}

test("picks a Workspace, shows its changes and diff, and remembers it for the Bot", async () => {
  const first = render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();

  await within(panel).findByRole("heading", { name: "Choose a workspace" });
  expect(testDevice.api.getWorkspaceGitChanges).not.toHaveBeenCalled();
  expect(within(panel).getByRole("button", { name: /gone/ })).toBeDisabled();
  await fireEvent.click(within(panel).getByRole("button", { name: /docs/ }));

  const row = await within(panel).findByRole("button", { name: "Staged: src/lib.rs" });
  expect(testDevice.api.getWorkspaceGitChanges).toHaveBeenCalledWith(
    "workspace-2",
    expect.anything(),
  );
  expect(await within(panel).findByText("main")).toBeVisible();
  expect(within(panel).getByRole("combobox", { name: "Workspace: docs" })).toHaveTextContent(
    "docs",
  );

  await fireEvent.click(row);
  await waitFor(() =>
    expect(panel.querySelector("[data-git-diff]")?.textContent).toContain("fn new() {}"),
  );
  // Esc leaves the diff, and on the list it leaves the side panel alone.
  await fireEvent.keyDown(panel.querySelector("[data-git-diff]")!, { key: "Escape" });
  await within(panel).findByRole("button", { name: "Staged: src/lib.rs" });
  await fireEvent.keyDown(within(panel).getByRole("button", { name: "Staged: src/lib.rs" }), {
    key: "Escape",
  });
  expect(screen.getByRole("complementary", { name: "Sidebar" })).toBeInTheDocument();

  // On wide screens the side panel stays open across a reload.
  first.unmount();
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const reopened = await screen.findByRole("complementary", { name: "Sidebar" });
  expect(within(reopened).getByRole("tab", { name: "Git" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(
    await within(reopened).findByRole("combobox", { name: "Workspace: docs" }),
  ).toHaveTextContent("docs");
});

test("shows the chosen Workspace's commit history and forgets the commit for another Workspace", async () => {
  vi.spyOn(testDevice.api, "getWorkspaceGitBranches").mockResolvedValue({
    head: "refs/heads/main",
    branches: [{ name: "main", ref: "refs/heads/main", remote: false, current: true }],
  });
  vi.spyOn(testDevice.api, "getWorkspaceGitCommits").mockResolvedValue({
    commits: [
      {
        sha: "a".repeat(40),
        parents: [],
        author_name: "Ada",
        authored_at: "2026-11-20T00:00:00Z",
        subject: "initial commit",
      },
    ],
    has_more: false,
  });
  vi.spyOn(testDevice.api, "getWorkspaceGitCommit").mockResolvedValue({
    sha: "a".repeat(40),
    parents: [],
    author_name: "Ada",
    author_email: "ada@example.com",
    authored_at: "2026-11-20T00:00:00Z",
    committer_name: "Ada",
    committed_at: "2026-11-20T00:00:00Z",
    subject: "initial commit",
    body: "",
    files: [{ path: "README.md", status: "A", additions: 1, deletions: 0, binary: false }],
    total: 1,
    truncated: false,
  });
  localStorage.setItem('pontia.botGitWorkspace:["local","a"]', "workspace-1");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();

  await fireEvent.click(await within(panel).findByRole("button", { name: /initial commit/ }));
  expect(await within(panel).findByRole("button", { name: "README.md" })).toBeVisible();
  expect(testDevice.api.getWorkspaceGitCommits).toHaveBeenCalledWith(
    "workspace-1",
    "refs/heads/main",
    0,
    expect.anything(),
  );

  await fireEvent.keyDown(panel.querySelector("[data-git-row]")!, { key: "Escape" });
  const picker = await within(panel).findByRole("combobox", { name: "Workspace: pontia" });
  await fireEvent.keyDown(picker, { key: "ArrowDown" });
  await screen.findByRole("option", { name: /docs/ });
  await fireEvent.keyDown(picker, { key: "ArrowDown" });
  await fireEvent.keyDown(picker, { key: "Enter" });
  await within(panel).findByRole("combobox", { name: "Workspace: docs" });
  // The history list stays visible; the commit from the other Workspace is gone.
  await within(panel).findByRole("button", { name: /initial commit/ });
  expect(within(panel).getByRole("button", { name: "History" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  expect(testDevice.api.getWorkspaceGitCommits).toHaveBeenLastCalledWith(
    "workspace-2",
    "refs/heads/main",
    0,
    expect.anything(),
  );
});

test("points to Workspace settings when none is registered", async () => {
  vi.mocked(testDevice.api.listWorkspaces).mockResolvedValue([]);
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  expect(await within(panel).findByText("No workspaces")).toBeVisible();
  expect(within(panel).getByRole("button", { name: "Manage workspaces" })).toBeVisible();
});

test("reads a saved Workspace only while the Git tab shows", async () => {
  history.replaceState({}, "", "?panel=search");
  localStorage.setItem('pontia.botGitWorkspace:["local","a"]', "workspace-1");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  await screen.findByRole("tab", { name: "Git" });
  expect(testDevice.api.listWorkspaces).not.toHaveBeenCalled();
  expect(testDevice.api.getWorkspaceGitChanges).not.toHaveBeenCalled();

  await fireEvent.click(screen.getByRole("tab", { name: "Git" }));
  const panel = screen.getByRole("complementary", { name: "Sidebar" });
  await within(panel).findByRole("button", { name: "Staged: src/lib.rs" });
  expect(testDevice.api.getWorkspaceGitChanges).toHaveBeenCalledWith(
    "workspace-1",
    expect.anything(),
  );

  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockClear();
  await fireEvent.click(screen.getByRole("tab", { name: "Search" }));
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(testDevice.api.getWorkspaceGitChanges).not.toHaveBeenCalled();
});

test("shows the picker for a saved Workspace that is unavailable", async () => {
  localStorage.setItem('pontia.botGitWorkspace:["local","a"]', "workspace-3");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  await within(panel).findByRole("heading", { name: "Choose a workspace" });
  expect(testDevice.api.getWorkspaceGitChanges).not.toHaveBeenCalled();
});

test("requires confirmation before discarding an unstaged file and updates the list", async () => {
  const file = { ...changes.files[0]!, group: "unstaged" as const };
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({ ...changes, files: [file] });
  const write = vi.spyOn(testDevice.api, "writeWorkspaceGit").mockImplementation(async () => {
    vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({
      ...changes,
      total: 0,
      files: [],
    });
    return { ...(await testDevice.api.refreshWorkspaceGitStatus("workspace-1")), clean: true };
  });
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  await fireEvent.click(await within(panel).findByRole("button", { name: /^pontia / }));
  const discard = await within(panel).findByRole("button", { name: "Discard changes: src/lib.rs" });
  await fireEvent.click(discard);
  await fireEvent.click(
    within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Cancel" }),
  );
  expect(write).not.toHaveBeenCalled();
  await fireEvent.click(discard);
  await fireEvent.click(
    within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Discard changes" }),
  );
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  await waitFor(() => expect(discard).not.toBeInTheDocument());
  expect(write).toHaveBeenCalledExactlyOnceWith("workspace-1", {
    action: "discard",
    body: { paths: ["src/lib.rs"] },
  });
});

test("keeps the commit draft after failure and sends one stage-all commit while pending", async () => {
  const file = { ...changes.files[0]!, group: "unstaged" as const };
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({ ...changes, files: [file] });
  const status = {
    ...(await testDevice.api.refreshWorkspaceGitStatus("workspace-1")),
    staged_count: 0,
    unstaged_count: 1,
  };
  vi.mocked(testDevice.api.refreshWorkspaceGitStatus).mockResolvedValue(status);
  const response = Promise.withResolvers<typeof status>();
  const write = vi
    .spyOn(testDevice.api, "writeWorkspaceGit")
    .mockRejectedValueOnce(new Error("Git identity missing"))
    .mockReturnValueOnce(response.promise);
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  await fireEvent.click(await within(panel).findByRole("button", { name: /^pontia / }));
  const input = await within(panel).findByRole("textbox", { name: "Commit message" });
  const commit = within(panel).getByRole("button", { name: "Commit" });
  await fireEvent.click(commit);
  expect(input).toHaveFocus();
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(write).not.toHaveBeenCalled();
  await fireEvent.input(input, { target: { value: "Save work" } });
  expect(input).toHaveAttribute("aria-invalid", "false");
  await fireEvent.click(commit);
  await waitFor(() => expect(within(panel).getByText("Git identity missing")).toBeInTheDocument());
  expect(input).toHaveValue("Save work");
  await fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
  expect(commit).toBeDisabled();
  await fireEvent.click(commit);
  expect(write).toHaveBeenCalledTimes(2);
  expect(write).toHaveBeenLastCalledWith("workspace-1", {
    action: "commit",
    body: { message: "Save work", stage_all: true },
  });
  response.resolve({ ...status, clean: true });
  await waitFor(() => expect(input).toHaveValue(""));
});

test("stops sync at a rejected fast-forward and keeps the failure dismissible", async () => {
  const status = { ...(await testDevice.api.refreshWorkspaceGitStatus("workspace-1")), behind: 1 };
  vi.mocked(testDevice.api.refreshWorkspaceGitStatus).mockResolvedValue(status);
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({
    ...changes,
    files: [],
    total: 0,
  });
  const write = vi
    .spyOn(testDevice.api, "writeWorkspaceGit")
    .mockRejectedValue(new Error("fatal: Not possible to fast-forward, aborting."));
  localStorage.setItem('pontia.botGitWorkspace:["local","a"]', "workspace-1");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  await fireEvent.click(await within(panel).findByRole("button", { name: /Sync Changes/ }));
  const error = await within(panel).findByRole("alert");
  expect(error).toHaveTextContent("fatal: Not possible to fast-forward, aborting.");
  expect(write).toHaveBeenCalledExactlyOnceWith("workspace-1", { action: "pull" });
  await fireEvent.click(within(error).getByRole("button", { name: "Close" }));
  expect(error).not.toBeInTheDocument();
});

test("syncs an incoming change before pushing and hides the primary action when up to date", async () => {
  const initial = { ...(await testDevice.api.refreshWorkspaceGitStatus("workspace-1")), behind: 1 };
  vi.mocked(testDevice.api.refreshWorkspaceGitStatus).mockResolvedValue(initial);
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({
    ...changes,
    files: [],
    total: 0,
  });
  const write = vi
    .spyOn(testDevice.api, "writeWorkspaceGit")
    .mockResolvedValueOnce({ ...initial, behind: 0 })
    .mockResolvedValueOnce({ ...initial, ahead: 0, behind: 0 });
  localStorage.setItem('pontia.botGitWorkspace:["local","a"]', "workspace-1");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  const sync = await within(panel).findByRole("button", { name: /Sync Changes/ });
  await fireEvent.click(sync);
  await waitFor(() => expect(sync).not.toBeInTheDocument());
  expect(write.mock.calls).toEqual([
    ["workspace-1", { action: "pull" }],
    ["workspace-1", { action: "push" }],
  ]);
});

test("offers push from the menu on a clean branch without an upstream", async () => {
  const status = {
    ...(await testDevice.api.refreshWorkspaceGitStatus("workspace-1")),
    upstream: null,
    ahead: 0,
  };
  vi.mocked(testDevice.api.refreshWorkspaceGitStatus).mockResolvedValue(status);
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({
    ...changes,
    files: [],
    total: 0,
  });
  const write = vi.spyOn(testDevice.api, "writeWorkspaceGit").mockResolvedValue(status);
  localStorage.setItem('pontia.botGitWorkspace:["local","a"]', "workspace-1");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  const menu = await within(panel).findByRole("button", { name: "More Git actions" });
  expect(within(panel).queryByRole("textbox", { name: "Commit message" })).toBeNull();
  await fireEvent.keyDown(menu, { key: "Enter" });
  await fireEvent.click(await screen.findByRole("menuitem", { name: "Push" }));
  await waitFor(() =>
    expect(write).toHaveBeenCalledExactlyOnceWith("workspace-1", { action: "push" }),
  );
});

test("blocks commit shortcuts while the workspace has conflicts", async () => {
  const file = { ...changes.files[0]!, group: "conflicted" as const, status: "U" };
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({ ...changes, files: [file] });
  const write = vi.spyOn(testDevice.api, "writeWorkspaceGit");
  localStorage.setItem('pontia.botGitWorkspace:["local","a"]', "workspace-1");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  const input = await within(panel).findByRole("textbox", { name: "Commit message" });
  await fireEvent.input(input, { target: { value: "Still resolving" } });
  await fireEvent.keyDown(input, { key: "Enter", ctrlKey: true });
  expect(within(panel).getByRole("button", { name: "Commit" })).toBeDisabled();
  expect(write).not.toHaveBeenCalled();
});

test("stages all from a truncated list whose visible files are staged", async () => {
  const status = {
    ...(await testDevice.api.refreshWorkspaceGitStatus("workspace-1")),
    unstaged_count: 1,
  };
  vi.mocked(testDevice.api.refreshWorkspaceGitStatus).mockResolvedValue(status);
  vi.mocked(testDevice.api.getWorkspaceGitChanges).mockResolvedValue({
    ...changes,
    total: 2,
    truncated: true,
  });
  const write = vi.spyOn(testDevice.api, "writeWorkspaceGit").mockResolvedValue(status);
  localStorage.setItem('pontia.botGitWorkspace:["local","a"]', "workspace-1");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  await fireEvent.keyDown(await within(panel).findByRole("button", { name: "More Git actions" }), {
    key: "Enter",
  });
  await fireEvent.click(await screen.findByRole("menuitem", { name: "Stage all" }));
  await waitFor(() =>
    expect(write).toHaveBeenCalledExactlyOnceWith("workspace-1", {
      action: "stage",
      body: { all: true },
    }),
  );
});
