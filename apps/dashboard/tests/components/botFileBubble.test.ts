import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import BotFileBubble from "../../src/components/bots/BotFileBubble.svelte";
import BotFilePreviewHarness from "./BotFilePreviewHarness.svelte";
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
});

test("shows the extension and size, and keeps the extension visible apart from the name", () => {
  render(BotFileBubble, { file, sessionId: "main" });
  expect(screen.getByText("ZIP · 3 MiB")).toBeInTheDocument();
  expect(screen.getByTitle(file.name)).toHaveTextContent("release-bundle-v1.8.0.zip");
  expect(screen.getByText(".zip")).toBeInTheDocument();
});

test("shows reception progress with a cancel button while downloading", async () => {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  vi.spyOn(testDevice.fileApi, "sharedFileContent").mockResolvedValue(
    new Response(new ReadableStream({ start: (controller) => void (stream = controller) }), {
      headers: { "Content-Length": String(file.size) },
    }),
  );
  render(BotFileBubble, { file, sessionId: "main" });
  await fireEvent.click(screen.getByRole("button", { name: `Download ${file.name}` }));
  stream.enqueue(new Uint8Array(1024 * 1024));
  await waitFor(() =>
    expect(screen.getByRole("status")).toHaveTextContent("Receiving… 1 MiB / 3 MiB"),
  );
  await fireEvent.click(screen.getByRole("button", { name: `Cancel download of ${file.name}` }));
  await waitFor(() => expect(get(downloads)).toEqual([]));
  expect(screen.getByRole("button", { name: `Download ${file.name}` })).toBeInTheDocument();
});

test("offers a retry when the download fails for a reason other than a missing file", async () => {
  vi.spyOn(testDevice.fileApi, "sharedFileContent").mockRejectedValue(
    new ApiError("File path has changed", "file_path_changed", 409),
  );
  render(BotFileBubble, { file, sessionId: "main" });
  await fireEvent.click(screen.getByRole("button", { name: `Download ${file.name}` }));
  expect(await screen.findByRole("alert")).toHaveTextContent("File path has changed");
  expect(
    screen.getByRole("button", { name: `Retry download of ${file.name}` }),
  ).toBeInTheDocument();
});

test("marks a deleted file as unavailable without a download action", async () => {
  vi.spyOn(testDevice.fileApi, "sharedFileContent").mockRejectedValue(
    new ApiError("File not found", "not_found", 404),
  );
  render(BotFileBubble, { file, sessionId: "main" });
  await fireEvent.click(screen.getByRole("button", { name: `Download ${file.name}` }));
  expect(await screen.findByRole("alert")).toHaveTextContent("File no longer exists");
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
  render(BotFileBubble, {
    file: { ...file, file_id: "gone", name: "gone.png", media_type: "image/png" },
    sessionId: "main",
  });
  expect(await screen.findByRole("alert")).toHaveTextContent("File no longer exists");
  expect(screen.queryByRole("img", { name: "gone.png" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
  vi.unstubAllGlobals();
});

test.each(["release-bundle-v1.8.0.zip", "demo.mp4", "paper.pdf"])(
  "%s offers only a download, without a preview",
  (name) => {
    render(BotFilePreviewHarness, { file: { ...file, name, path: `/work/${name}` } });
    expect(screen.queryByRole("button", { name: `Preview ${name}` })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Download ${name}` })).toBeInTheDocument();
  },
);

test.each(["notes.md", "Makefile"])("%s opens in the preview panel", async (name) => {
  const read = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValue(new Response("plain text"));
  render(BotFilePreviewHarness, { file: { ...file, name, path: `/work/${name}` } });
  await fireEvent.click(screen.getByRole("button", { name: `Preview ${name}` }));
  await within(screen.getByRole("complementary")).findByText("plain text");
  expect(read).toHaveBeenCalledWith("main", `/work/${name}`, expect.any(AbortSignal));
});
