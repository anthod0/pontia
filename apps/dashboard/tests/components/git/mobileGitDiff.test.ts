import { fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, expect, test, vi } from "vitest";
import type { GitChangedFile } from "../../../src/api/types";
import MobileGitDiffScreen from "../../../src/views/mobile/sessions/MobileGitDiffScreen.svelte";
import { testDevice } from "../../deviceFixture";

afterEach(() => {
  testDevice.queryClient.clear();
  vi.restoreAllMocks();
});

test("opens the unstaged diff of a partially staged file and navigates to its staged diff", async () => {
  const staged: GitChangedFile = {
    path: "notes.txt",
    group: "staged",
    status: "M",
    additions: 1,
    deletions: 0,
    binary: false,
  };
  const unstaged: GitChangedFile = { ...staged, group: "unstaged" };
  vi.spyOn(testDevice.api, "getWorkspaceGitDiff").mockImplementation(async (_, file) => ({
    ...file,
    binary: false,
    truncated: false,
    hunks: [
      {
        header: "@@ -0,0 +1 @@",
        lines: [
          {
            kind: "add",
            new: 1,
            old: null,
            text: file.group === "staged" ? "index" : "working tree",
          },
        ],
      },
    ],
  }));
  render(MobileGitDiffScreen, {
    props: {
      device: testDevice,
      workspaceId: "workspace",
      target: { file: unstaged },
      files: [staged, unstaged],
    },
  });
  expect(await screen.findByText("working tree")).toBeVisible();
  await fireEvent.click(screen.getByRole("button", { name: /^Previous file/ }));
  expect(await screen.findByText("index")).toBeVisible();
});
