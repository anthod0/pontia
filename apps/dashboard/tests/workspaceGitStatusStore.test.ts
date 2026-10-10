import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";
import { queryClient } from "../src/queries/queryClient";
import WorkspaceGitStatusQueryHarness from "./components/WorkspaceGitStatusQueryHarness.svelte";

const mocks = vi.hoisted(() => ({
  getWorkspaceGitStatus: vi.fn(),
  refreshWorkspaceGitStatus: vi.fn(),
}));

vi.mock("../src/api/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../src/api/client")>()),
  getWorkspaceGitStatus: mocks.getWorkspaceGitStatus,
  refreshWorkspaceGitStatus: mocks.refreshWorkspaceGitStatus,
}));

beforeEach(() => {
  queryClient.clear();
  mocks.getWorkspaceGitStatus.mockReset().mockResolvedValue({
    workspace_id: "workspace-1",
    state: "unknown",
    observed_at: null,
  });
  mocks.refreshWorkspaceGitStatus.mockReset().mockResolvedValue({
    workspace_id: "workspace-1",
    state: "observed",
    branch: "main",
    clean: false,
    observed_at: "now",
  });
});

test("refreshing git status replaces the cached workspace status", async () => {
  render(WorkspaceGitStatusQueryHarness, { workspaceId: "workspace-1" });
  expect(await screen.findByText("unknown")).toBeInTheDocument();

  await fireEvent.click(screen.getByRole("button", { name: "Refresh" }));

  expect(await screen.findByText("observed")).toBeInTheDocument();
  expect(mocks.refreshWorkspaceGitStatus).toHaveBeenCalledWith("workspace-1");
});
