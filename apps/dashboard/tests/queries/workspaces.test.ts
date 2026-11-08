import { testDevice } from "../deviceFixture";
import { beforeEach, expect, test, vi } from "vitest";
import type { WorkspaceGitStatusView } from "../../src/api/types";
const { queryClient } = testDevice;
const { refreshWorkspaceGitStatus, workspaceKeys } = testDevice.workspaceQueries;

const refreshedStatus: WorkspaceGitStatusView = {
  workspace_id: "workspace-1",
  repo_root: "/repo",
  branch: "main",
  upstream: "origin/main",
  ahead: 0,
  behind: 0,
  staged_count: 0,
  unstaged_count: 1,
  untracked_count: 0,
  conflicted_count: 0,
  clean: false,
  state: "observed",
  failure: null,
  observed_at: "2026-05-14T00:00:00Z",
  updated_at: "2026-05-14T00:00:00Z",
};

const requestRefreshWorkspaceGitStatus = vi.hoisted(() => vi.fn(async () => refreshedStatus));

vi.mock("../../src/api/client", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    refreshWorkspaceGitStatus: requestRefreshWorkspaceGitStatus,
  };
  return {
    ...original,
    ...mocked,
    createApi: (device) => ({ ...original.createApi(device), ...mocked }),
  };
});

beforeEach(() => {
  queryClient.clear();
  requestRefreshWorkspaceGitStatus.mockClear();
});

test("a Git status refresh stores the confirmed server snapshot in the Query cache", async () => {
  await refreshWorkspaceGitStatus("workspace-1");

  expect(requestRefreshWorkspaceGitStatus).toHaveBeenCalledWith("workspace-1");
  expect(queryClient.getQueryData(workspaceKeys.gitStatus("workspace-1"))).toEqual(refreshedStatus);
});
