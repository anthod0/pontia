import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SharedFilePreviewHarness from "./SharedFilePreviewHarness.svelte";
import { get } from "svelte/store";
import { downloads } from "../../src/lib/downloads/manager";
import { testDevice } from "../deviceFixture";
import { ApiError } from "../../src/api/errors";
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

test("lists a shared file the preview panel cannot show without a preview action", () => {
  render(SharedFilePreviewHarness, {
    sharedFiles: [
      {
        file_id: "file-1",
        turn_id: "t1",
        name: "demo.mp4",
        path: "/tmp/out/demo.mp4",
        size: 8,
        media_type: "video/mp4",
        created_at: "2026-06-11T00:00:05Z",
      },
    ],
  });
  expect(screen.queryByRole("button", { name: "Preview demo.mp4" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Download demo.mp4" })).toBeInTheDocument();
});

test("a missing shared text file loses its download and preview actions in Session chat", async () => {
  const unregister = downloads.registerDevice(testDevice);
  vi.spyOn(testDevice.fileApi, "sharedFileContent").mockRejectedValue(
    new ApiError("File not found", "not_found", 404),
  );
  try {
    render(SharedFilePreviewHarness, {
      sharedFiles: [
        {
          file_id: "missing",
          turn_id: "t1",
          name: "missing.txt",
          path: "/tmp/missing.txt",
          size: 10,
          media_type: "text/plain",
          created_at: "2026-11-20T00:00:00Z",
        },
      ],
    });
    expect(screen.getByRole("button", { name: "Preview missing.txt" })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Download missing.txt" }));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Preview missing.txt" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Download missing.txt" })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Retry download of missing.txt" }),
    ).not.toBeInTheDocument();
  } finally {
    for (const task of get(downloads)) downloads.cancel(task.id);
    unregister();
  }
});
