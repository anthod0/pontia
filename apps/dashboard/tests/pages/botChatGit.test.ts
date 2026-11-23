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

test("points to Workspace settings when none is registered", async () => {
  vi.mocked(testDevice.api.listWorkspaces).mockResolvedValue([]);
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  const panel = await openGitTab();
  expect(await within(panel).findByText("No workspaces")).toBeVisible();
  expect(within(panel).getByRole("button", { name: "Manage workspaces" })).toBeVisible();
});

test("reads a saved Workspace only while the Git tab shows", async () => {
  localStorage.setItem("pontia.botPanelTab", "search");
  localStorage.setItem('pontia.botGitWorkspace:["local","a"]', "workspace-1");
  render(BotChatPreviewHarness, { botId: bot.bot_id });
  await fireEvent.click(await screen.findByRole("button", { name: "Sidebar" }));
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
