import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SharedFilePreviewHarness from "./SharedFilePreviewHarness.svelte";
import { get } from "svelte/store";
import { downloads } from "../../src/lib/downloads/manager";
import { testDevice } from "../deviceFixture";
import { token } from "../../src/stores/auth";
import { VisibleObserver } from "./fileCardFixture";

beforeEach(() => {
  token.set("shared-preview-token");
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  token.set("");
});

test("an attached text file opens in the file preview panel", async () => {
  const fetch = vi.fn().mockResolvedValue(new Response("# Notes\n"));
  vi.stubGlobal("fetch", fetch);
  render(SharedFilePreviewHarness, { paths: ["/tmp/out/notes.md"] });
  expect(fetch).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole("button", { name: "Preview notes.md" }));
  const panel = screen.getByRole("complementary");
  await within(panel).findByRole("heading", { name: "Notes" });
  const request = new URL(fetch.mock.calls[0][0], "http://localhost");
  expect(request.pathname).toBe("/api/v1/sessions/session-1/files/content");
  expect(request.searchParams.get("path")).toBe("/tmp/out/notes.md");
});

test("a card reads each file's details with a HEAD request on the path route", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  const fetch = vi.fn().mockResolvedValue(
    new Response(null, {
      headers: { "Content-Length": "2048", "Content-Type": "application/pdf" },
    }),
  );
  vi.stubGlobal("fetch", fetch);
  render(SharedFilePreviewHarness, { paths: ["/tmp/out/report.pdf"] });
  expect(await screen.findByText("PDF · 2 KiB")).toBeInTheDocument();
  const [url, init] = fetch.mock.calls[0];
  expect(init.method).toBe("HEAD");
  expect(new URL(url, "http://localhost").searchParams.get("path")).toBe("/tmp/out/report.pdf");
});

test("an attached file the preview panel cannot show has no preview action", () => {
  render(SharedFilePreviewHarness, { paths: ["/tmp/out/demo.mp4"] });
  expect(screen.queryByRole("button", { name: "Preview demo.mp4" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Download demo.mp4" })).toBeInTheDocument();
});

test("a missing attached file loses its download and preview actions", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  const unregister = downloads.registerDevice(testDevice);
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(new Response(null, { status: 404, statusText: "Not Found" })),
  );
  try {
    render(SharedFilePreviewHarness, { paths: ["/tmp/missing.txt"] });
    expect(await screen.findByRole("alert")).toHaveTextContent("File no longer exists");
    expect(screen.queryByRole("button", { name: "Preview missing.txt" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Download missing.txt" })).not.toBeInTheDocument();
  } finally {
    for (const task of get(downloads)) downloads.cancel(task.id);
    unregister();
  }
});
