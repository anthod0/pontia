import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import FileBubble from "../../src/components/chat/FileBubble.svelte";
import FileBubblePreviewHarness from "./FileBubblePreviewHarness.svelte";
import { downloads } from "../../src/lib/downloads/manager";
import { testDevice } from "../deviceFixture";
import { ApiError } from "../../src/api/errors";
import type { SharedFile } from "../../src/api/types";

const file: SharedFile = {
  file_id: "file-1",
  turn_id: null,
  name: "release-bundle-v1.8.0.zip",
  path: "/work/release-bundle-v1.8.0.zip",
  size: 3 * 1024 * 1024,
  media_type: "application/octet-stream",
  created_at: "2026-11-20T00:00:00Z",
};
let unregister: () => void;

beforeEach(() => {
  unregister = downloads.registerDevice(testDevice);
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:bot-file");
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
});

afterEach(() => {
  for (const task of get(downloads)) downloads.cancel(task.id);
  unregister();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

test("shows reception progress with a cancel button while downloading", async () => {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  vi.spyOn(testDevice.fileApi, "sharedFileContent").mockResolvedValue(
    new Response(new ReadableStream({ start: (controller) => void (stream = controller) }), {
      headers: { "Content-Length": String(file.size) },
    }),
  );
  render(FileBubble, { file, sessionId: "main" });
  await fireEvent.click(screen.getByRole("button", { name: `Download ${file.name}` }));
  stream.enqueue(new Uint8Array(1024 * 1024));
  await waitFor(() =>
    expect(get(downloads)[0].progress).toEqual({ received: 1024 * 1024, total: file.size }),
  );
  expect(screen.getByRole("status")).toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: `Cancel download of ${file.name}` }));
  await waitFor(() => expect(get(downloads)).toEqual([]));
  expect(screen.getByRole("button", { name: `Download ${file.name}` })).toBeInTheDocument();
});

test("retries a failed download and saves it successfully", async () => {
  vi.spyOn(testDevice.fileApi, "sharedFileContent")
    .mockRejectedValueOnce(new ApiError("File path has changed", "file_path_changed", 409))
    .mockResolvedValueOnce(new Response("archive"));
  render(FileBubble, { file, sessionId: "main" });
  await fireEvent.click(screen.getByRole("button", { name: `Download ${file.name}` }));
  expect(await screen.findByRole("alert")).toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: `Retry download of ${file.name}` }));
  await waitFor(() => expect(HTMLAnchorElement.prototype.click).toHaveBeenCalledTimes(1));
  expect(get(downloads)).toEqual([]);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("marks a deleted file as unavailable without a download action", async () => {
  vi.spyOn(testDevice.fileApi, "sharedFileContent").mockRejectedValue(
    new ApiError("File not found", "not_found", 404),
  );
  render(FileBubble, { file, sessionId: "main" });
  await fireEvent.click(screen.getByRole("button", { name: `Download ${file.name}` }));
  expect(await screen.findByRole("alert")).toBeInTheDocument();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
});

test("falls back to the file bubble when a shared image is gone", async () => {
  class VisibleObserver {
    constructor(private readonly callback: IntersectionObserverCallback) {}
    observe(target: Element) {
      this.callback([{ isIntersecting: true, target } as IntersectionObserverEntry], this as never);
    }
    disconnect() {}
  }
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(testDevice.fileApi, "sharedFileContent").mockRejectedValue(
    new ApiError("File not found", "not_found", 404),
  );
  render(FileBubble, {
    file: { ...file, file_id: "gone", name: "gone.png", media_type: "image/png" },
    sessionId: "main",
  });
  expect(await screen.findByRole("alert")).toBeInTheDocument();
  expect(screen.queryByRole("img", { name: "gone.png" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
  vi.unstubAllGlobals();
});

test.each(["release-bundle-v1.8.0.zip", "demo.mp4", "paper.pdf"])(
  "%s offers only a download, without a preview",
  (name) => {
    render(FileBubblePreviewHarness, { file: { ...file, name, path: `/work/${name}` } });
    expect(screen.queryByRole("button", { name: `Preview ${name}` })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Download ${name}` })).toBeInTheDocument();
  },
);

test.each(["notes.md", "Makefile"])("%s opens in the preview panel", async (name) => {
  const read = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValue(new Response("plain text"));
  render(FileBubblePreviewHarness, { file: { ...file, name, path: `/work/${name}` } });
  await fireEvent.click(screen.getByRole("button", { name: `Preview ${name}` }));
  await within(screen.getByRole("complementary")).findByText("plain text");
  expect(read).toHaveBeenCalledWith("main", `/work/${name}`, expect.any(AbortSignal));
});
