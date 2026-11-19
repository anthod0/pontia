import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SharedFilePreviewHarness from "./SharedFilePreviewHarness.svelte";
import { token } from "../../src/stores/auth";

beforeEach(() => {
  token.set("shared-preview-token");
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  token.set("");
});

test("previews a shared text file in the file preview panel", async () => {
  const fetch = vi.fn().mockResolvedValue(new Response("# Notes\n"));
  vi.stubGlobal("fetch", fetch);
  render(SharedFilePreviewHarness, {
    sharedFiles: [
      {
        file_id: "file-1",
        turn_id: "t1",
        name: "notes.md",
        path: "/tmp/out/notes.md",
        size: 8,
        media_type: "application/octet-stream",
        created_at: "2026-06-11T00:00:05Z",
      },
    ],
  });
  expect(fetch).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole("button", { name: "Preview notes.md" }));
  const panel = screen.getByRole("complementary");
  await within(panel).findByRole("heading", { name: "Notes" });
  const request = new URL(fetch.mock.calls[0][0], "http://localhost");
  expect(request.pathname).toBe("/api/v1/sessions/session-1/files/content");
  expect(request.searchParams.get("path")).toBe("/tmp/out/notes.md");
});
