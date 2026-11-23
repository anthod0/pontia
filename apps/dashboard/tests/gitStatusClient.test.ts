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
