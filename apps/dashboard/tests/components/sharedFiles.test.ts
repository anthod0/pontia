import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SharedFilesHarness from "./SharedFilesHarness.svelte";
import { downloads } from "../../src/lib/downloads/manager";
import { testDevice } from "../deviceFixture";
import { ApiError } from "../../src/api/errors";
import type { SharedFile } from "../../src/api/types";

const shared = (name: string, extra: Partial<SharedFile> = {}): SharedFile => ({
  file_id: `file-${name}`,
  share_id: "share",
  turn_id: null,
  name,
  path: `/work/${name}`,
  size: 1024,
  media_type: "application/octet-stream",
  created_at: "2026-11-20T00:00:00Z",
  ...extra,
});
const image = (name: string, extra: Partial<SharedFile> = {}) =>
  shared(name, { media_type: "image/png", ...extra });

class VisibleObserver {
  constructor(private readonly callback: IntersectionObserverCallback) {}
  observe(target: Element) {
    this.callback([{ isIntersecting: true, target } as IntersectionObserverEntry], this as never);
  }
  disconnect() {}
}
let unregister: () => void;

beforeEach(() => {
  unregister = downloads.registerDevice(testDevice);
  vi.spyOn(URL, "createObjectURL").mockImplementation(() => `blob:${crypto.randomUUID()}`);
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  Element.prototype.scrollIntoView = vi.fn();
});

afterEach(() => {
  for (const task of get(downloads)) downloads.cancel(task.id);
  unregister();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
});

test("lists a mixed share under one header with the total size", () => {
  render(SharedFilesHarness, {
    files: [shared("report.pdf", { size: 2048 }), image("chart.png"), shared("data.csv")],
  });
  const card = document.querySelector<HTMLElement>("[data-shared-files]")!;
  expect(within(card).getByText(/^3 files · 4 KiB/)).toBeInTheDocument();
  expect(card.querySelectorAll("[data-file-row]")).toHaveLength(3);
  expect(within(card).getByRole("button", { name: "Download chart.png" })).toBeInTheDocument();
});

test("shows the first four of more than six files until expanded", async () => {
  const files = Array.from({ length: 11 }, (_, i) => shared(`notes-${i + 1}.md`));
  render(SharedFilesHarness, { files });
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(4);
  await fireEvent.click(screen.getByRole("button", { name: "Show 7 more" }));
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(11);
  await fireEvent.click(screen.getByRole("button", { name: "Show less" }));
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(4);
});

test("shows six files without collapsing", () => {
  render(SharedFilesHarness, {
    files: Array.from({ length: 6 }, (_, i) => shared(`notes-${i + 1}.md`)),
  });
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(6);
  expect(screen.queryByRole("button", { name: /Show \d+ more/ })).not.toBeInTheDocument();
});

test("expands a collapsed list to reveal a highlighted file", async () => {
  const files = Array.from({ length: 8 }, (_, i) => shared(`notes-${i + 1}.md`));
  render(SharedFilesHarness, { files, highlighted: "file-notes-7.md" });
  await waitFor(() => expect(document.querySelectorAll("[data-file-row]")).toHaveLength(8));
  await waitFor(() => expect(Element.prototype.scrollIntoView).toHaveBeenCalled());
  expect(document.querySelector('[data-shared-file="file-notes-7.md"]')!.className).toContain(
    "ring-2",
  );
});

test("downloads each file separately and skips files that are gone", async () => {
  const read = vi
    .spyOn(testDevice.fileApi, "sharedFileContent")
    .mockImplementation(async (_session, id) => {
      if (id === "file-gone.txt") throw new ApiError("File not found", "not_found", 404);
      return new Response(new ReadableStream());
    });
  render(SharedFilesHarness, { files: [shared("a.txt"), shared("gone.txt"), shared("b.txt")] });
  const all = screen.getByRole("button", { name: "Download all 3 files" });
  await fireEvent.click(all);
  await waitFor(() => expect(get(downloads)).toHaveLength(3));
  expect(read).toHaveBeenCalledTimes(3);
  await waitFor(() =>
    expect(get(downloads).find((task) => task.name === "gone.txt")?.errorCode).toBe("not_found"),
  );
  expect(screen.queryByRole("button", { name: "Download gone.txt" })).not.toBeInTheDocument();
  for (const task of get(downloads)) downloads.cancel(task.id);
  read.mockClear();
  await fireEvent.click(all);
  await waitFor(() => expect(read).toHaveBeenCalledTimes(3));
});

test("disables Download all when every file is gone", async () => {
  vi.spyOn(testDevice.fileApi, "sharedFileContent").mockRejectedValue(
    new ApiError("File not found", "not_found", 404),
  );
  render(SharedFilesHarness, { files: [shared("a.txt"), shared("b.txt")] });
  const all = screen.getByRole("button", { name: "Download all 2 files" });
  await fireEvent.click(all);
  await waitFor(() => expect(all).toBeDisabled());
});

test.each([
  [2, 2],
  [3, 3],
  [4, 4],
  [5, 5],
  [20, 5],
])("lays out %i images with %i tiles", async (count, tiles) => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(testDevice.previews, "sharedImageUrl").mockResolvedValue("blob:image");
  render(SharedFilesHarness, {
    files: Array.from({ length: count }, (_, i) => image(`shot-${i + 1}.png`)),
  });
  expect(screen.getByText(`${count} images`)).toBeInTheDocument();
  await waitFor(() =>
    expect(document.querySelectorAll("[data-shared-file] img")).toHaveLength(tiles),
  );
  if (count > 5) {
    await fireEvent.click(screen.getByRole("button", { name: `Show ${count - 5} more images` }));
    await waitFor(() =>
      expect(document.querySelectorAll("[data-shared-file] img")).toHaveLength(count),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Show less" }));
    expect(document.querySelectorAll("[data-shared-file] img")).toHaveLength(5);
  }
});

test("uses the list when any file is not a previewable image", () => {
  render(SharedFilesHarness, {
    files: [image("a.png"), image("big.png", { size: 21 * 1024 * 1024 })],
  });
  expect(screen.getByText(/^2 files/)).toBeInTheDocument();
  expect(screen.queryByText("2 images")).not.toBeInTheDocument();
});

test("switches an image share to its file list and back", async () => {
  render(SharedFilesHarness, { files: [image("a.png"), image("b.png")] });
  await fireEvent.click(screen.getByRole("button", { name: "Show as list" }));
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(2);
  await fireEvent.click(screen.getByRole("button", { name: "Show as images" }));
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(0);
});

test("shows reception progress and a retry on image tiles", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(testDevice.previews, "sharedImageUrl").mockResolvedValue("blob:image");
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  vi.spyOn(testDevice.fileApi, "sharedFileContent")
    .mockResolvedValueOnce(
      new Response(new ReadableStream({ start: (controller) => void (stream = controller) }), {
        headers: { "Content-Length": "1000" },
      }),
    )
    .mockRejectedValueOnce(new ApiError("Path changed", "file_path_changed", 409));
  render(SharedFilesHarness, { files: [image("a.png"), image("b.png")] });
  await screen.findAllByRole("button", { name: /Enlarge/ });
  await fireEvent.click(screen.getByRole("button", { name: "Download all 2 files" }));
  stream.enqueue(new Uint8Array(400));
  expect(await screen.findByText("40%")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Cancel download of a.png" })).toBeInTheDocument();
  expect(
    await screen.findByRole("button", { name: "Retry download of b.png" }),
  ).toBeInTheDocument();
  expect(screen.getByText(/1 receiving/)).toBeInTheDocument();
  expect(screen.getByText(/1 failed/)).toBeInTheDocument();
});

test("opens the viewer within the share, skipping images that are gone", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(testDevice.previews, "sharedImageUrl").mockImplementation(async (_session, id) => {
    if (id === "file-gone.png") throw new ApiError("File not found", "not_found", 404);
    return `blob:${id}`;
  });
  render(SharedFilesHarness, { files: [image("a.png"), image("gone.png"), image("c.png")] });
  await fireEvent.click(await screen.findByRole("button", { name: "Enlarge c.png" }));
  const viewer = await screen.findByRole("dialog");
  expect(within(viewer).getByRole("heading", { name: "c.png", hidden: true })).toBeInTheDocument();
  await fireEvent.click(within(viewer).getByRole("button", { name: "Previous image" }));
  expect(within(viewer).getByRole("heading", { name: "a.png", hidden: true })).toBeInTheDocument();
  expect(within(viewer).getByRole("button", { name: "Previous image" })).toBeDisabled();
});

test("expands the mosaic from the +N tile even when its image is gone", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(testDevice.previews, "sharedImageUrl").mockImplementation(async (_session, id) => {
    if (id === "file-shot-5.png") throw new ApiError("File not found", "not_found", 404);
    return `blob:${id}`;
  });
  render(SharedFilesHarness, {
    files: Array.from({ length: 7 }, (_, i) => image(`shot-${i + 1}.png`)),
  });
  await fireEvent.click(await screen.findByRole("button", { name: "Show 2 more images" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Enlarge shot-7.png" })).toBeInTheDocument(),
  );
});

test("expands the mosaic to reveal a highlighted image under the +N tile", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(testDevice.previews, "sharedImageUrl").mockResolvedValue("blob:image");
  render(SharedFilesHarness, {
    files: Array.from({ length: 7 }, (_, i) => image(`shot-${i + 1}.png`)),
    highlighted: "file-shot-5.png",
  });
  expect(await screen.findByRole("button", { name: "Enlarge shot-5.png" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Show less" })).toBeInTheDocument();
});
