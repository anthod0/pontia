import { get } from "svelte/store";
import type { GitWriteOperation } from "../src/api/types";
import { testDevice } from "./deviceFixture";
import { describe, expect, test, vi } from "vitest";
const { refreshWorkspaceGitStatus } = testDevice.api;

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("workspace git status API client", () => {
  test("refreshes workspace git status through External API", async () => {
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      if (url.endsWith("/workspaces/workspace-1/git-status/refresh") && init?.method === "POST") {
        return jsonResponse({
          data: {
            git_status: {
              workspace_id: "workspace-1",
              state: "observed",
              branch: "main",
              clean: true,
              observed_at: "now",
            },
          },
        });
      }
      throw new Error(`unexpected request ${url}`);
    });
    vi.stubGlobal("fetch", fetchMock);

    expect(await refreshWorkspaceGitStatus("workspace-1")).toMatchObject({
      branch: "main",
      clean: true,
    });
  });
});

describe("workspace git changes API client", () => {
  test("requests one file's diff with its group and original path", async () => {
    const fetchMock = vi.fn(async (url: string) => {
      expect(url).toBe(
        "/api/v1/workspaces/workspace-1/git/diff?path=docs%2Fnew+name.md&group=staged&orig_path=docs%2Fold.md",
      );
      return jsonResponse({ data: { diff: { path: "docs/new name.md", hunks: [] } } });
    });
    vi.stubGlobal("fetch", fetchMock);

    expect(
      await testDevice.api.getWorkspaceGitDiff("workspace-1", {
        path: "docs/new name.md",
        orig_path: "docs/old.md",
        group: "staged",
      }),
    ).toMatchObject({ path: "docs/new name.md" });
  });
});

describe("workspace Git writes", () => {
  test.each([
    { action: "stage", body: { all: true } },
    { action: "unstage", body: { paths: ["old name", "new name"] } },
    { action: "discard", body: { paths: ["draft"] } },
    { action: "commit", body: { message: "Add draft", stage_all: true } },
    { action: "pull" },
    { action: "push" },
  ] satisfies GitWriteOperation[])(
    "posts $action and applies the returned workspace status",
    async (operation) => {
      const status = { workspace_id: "space/one", branch: "main", ahead: 3 };
      const fetchMock = vi.fn(async () => jsonResponse({ data: { git_status: status } }));
      vi.stubGlobal("fetch", fetchMock);
      await testDevice.workspaceQueries.writeWorkspaceGit("space/one", operation);
      expect(fetchMock).toHaveBeenCalledExactlyOnceWith(
        `/api/v1/workspaces/space%2Fone/git/${operation.action}`,
        expect.objectContaining({ method: "POST", body: JSON.stringify(operation.body) }),
      );
      const statuses = testDevice.workspaceQueries.createWorkspaceGitStatusesStore();
      expect(get(statuses)["space/one"]).toEqual(status);
    },
  );
});
