import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SharedFilesHarness from "./SharedFilesHarness.svelte";
import { downloads } from "../../src/lib/downloads/manager";
import { testDevice } from "../deviceFixture";
import { ApiError } from "../../src/api/errors";
import { VisibleObserver, mockFileInfo } from "./fileCardFixture";

const work = (...names: string[]) => names.map((name) => `/work/${name}`);
let unregister: () => void;

beforeEach(() => {
  unregister = downloads.registerDevice(testDevice);
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(URL, "createObjectURL").mockImplementation(() => `blob:${crypto.randomUUID()}`);
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  vi.spyOn(testDevice.previews, "imageUrl").mockImplementation(
    async (_session, path) => `blob:${path}`,
  );
  Element.prototype.scrollIntoView = vi.fn();
});

afterEach(() => {
  for (const task of get(downloads)) downloads.cancel(task.id);
  unregister();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
});

test("lists mixed files under one header with the total size once known", async () => {
  mockFileInfo({ "/work/report.pdf": { size: 2048 } });
  render(SharedFilesHarness, { paths: work("report.pdf", "chart.png", "data.csv") });
  const card = document.querySelector<HTMLElement>("[data-shared-files]")!;
  expect(await within(card).findByText(/^3 files · 4 KiB/)).toBeInTheDocument();
  expect(card.querySelectorAll("[data-file-row]")).toHaveLength(3);
  expect(within(card).getByRole("button", { name: "Download chart.png" })).toBeInTheDocument();
  expect(within(card).getByText("PDF · 2 KiB")).toBeInTheDocument();
});

test("reads file details only when the card scrolls into view", () => {
  vi.unstubAllGlobals();
  const info = mockFileInfo();
  render(SharedFilesHarness, { paths: work("a.txt", "b.txt") });
  expect(screen.getByText(/^2 files$/)).toBeInTheDocument();
  expect(info).not.toHaveBeenCalled();
});

test("shows the first four of more than six files until expanded", async () => {
  mockFileInfo();
  render(SharedFilesHarness, {
    paths: Array.from({ length: 11 }, (_, i) => `/work/notes-${i + 1}.md`),
  });
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(4);
  await fireEvent.click(screen.getByRole("button", { name: "Show 7 more" }));
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(11);
  await fireEvent.click(screen.getByRole("button", { name: "Show less" }));
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(4);
});

test("shows six files without collapsing", () => {
  mockFileInfo();
  render(SharedFilesHarness, {
    paths: Array.from({ length: 6 }, (_, i) => `/work/notes-${i + 1}.md`),
  });
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(6);
  expect(screen.queryByRole("button", { name: /Show \d+ more/ })).not.toBeInTheDocument();
});

test("expands a collapsed list to reveal a highlighted file", async () => {
  mockFileInfo();
  const paths = Array.from({ length: 8 }, (_, i) => `/work/notes-${i + 1}.md`);
  render(SharedFilesHarness, { paths, highlighted: "/work/notes-7.md" });
  await waitFor(() => expect(document.querySelectorAll("[data-file-row]")).toHaveLength(8));
  await waitFor(() => expect(Element.prototype.scrollIntoView).toHaveBeenCalled());
  expect(document.querySelector('[data-shared-file="/work/notes-7.md"]')!.className).toContain(
    "ring-2",
  );
});

test("marks files that are gone and skips them in Download all", async () => {
  mockFileInfo({ "/work/gone.txt": null });
  const read = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValue(new Response(new ReadableStream()));
  render(SharedFilesHarness, { paths: work("a.txt", "gone.txt", "b.txt") });
  expect(await screen.findByText(/1 gone|File no longer exists/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Download gone.txt" })).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: "Download all 2 files" }));
  await waitFor(() => expect(read).toHaveBeenCalledTimes(2));
  expect(read.mock.calls.map(([, path]) => path)).toEqual(work("a.txt", "b.txt"));
});

test("disables Download all when every file is gone", async () => {
  mockFileInfo({ "/work/a.txt": null, "/work/b.txt": null });
  render(SharedFilesHarness, { paths: work("a.txt", "b.txt") });
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Download all 0 files" })).toBeDisabled(),
  );
});

test.each([
  [2, 2],
  [3, 3],
  [4, 4],
  [5, 5],
  [20, 5],
])("lays out %i images with %i tiles", async (count, tiles) => {
  mockFileInfo();
  render(SharedFilesHarness, {
    paths: Array.from({ length: count }, (_, i) => `/work/shot-${i + 1}.png`),
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

test("uses the list once any file turns out not to be a previewable image", async () => {
  mockFileInfo({ "/work/big.png": { size: 21 * 1024 * 1024 } });
  render(SharedFilesHarness, { paths: work("a.png", "big.png") });
  expect(await screen.findByText(/^2 files/)).toBeInTheDocument();
  expect(screen.queryByText("2 images")).not.toBeInTheDocument();
});

test("switches an image card to its file list and back", async () => {
  mockFileInfo();
  render(SharedFilesHarness, { paths: work("a.png", "b.png") });
  await fireEvent.click(screen.getByRole("button", { name: "Show as list" }));
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(2);
  await fireEvent.click(screen.getByRole("button", { name: "Show as images" }));
  expect(document.querySelectorAll("[data-file-row]")).toHaveLength(0);
});

test("shows reception progress and a retry on image tiles", async () => {
  mockFileInfo();
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  vi.spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValueOnce(
      new Response(new ReadableStream({ start: (controller) => void (stream = controller) }), {
        headers: { "Content-Length": "1000" },
      }),
    )
    .mockRejectedValueOnce(new ApiError("Read failed", "internal_error", 500));
  render(SharedFilesHarness, { paths: work("a.png", "b.png") });
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

test("opens the viewer within the card, skipping images that are gone", async () => {
  mockFileInfo({ "/work/gone.png": null });
  render(SharedFilesHarness, { paths: work("a.png", "gone.png", "c.png") });
  await fireEvent.click(await screen.findByRole("button", { name: "Enlarge c.png" }));
  const viewer = await screen.findByRole("dialog");
  expect(within(viewer).getByRole("heading", { name: "c.png", hidden: true })).toBeInTheDocument();
  await fireEvent.click(within(viewer).getByRole("button", { name: "Previous image" }));
  expect(within(viewer).getByRole("heading", { name: "a.png", hidden: true })).toBeInTheDocument();
  expect(within(viewer).getByRole("button", { name: "Previous image" })).toBeDisabled();
});

test("expands the mosaic from the +N tile even when its image is gone", async () => {
  mockFileInfo({ "/work/shot-5.png": null });
  render(SharedFilesHarness, {
    paths: Array.from({ length: 7 }, (_, i) => `/work/shot-${i + 1}.png`),
  });
  await fireEvent.click(await screen.findByRole("button", { name: "Show 2 more images" }));
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Enlarge shot-7.png" })).toBeInTheDocument(),
  );
});

test("expands the mosaic to reveal a highlighted image under the +N tile", async () => {
  mockFileInfo();
  render(SharedFilesHarness, {
    paths: Array.from({ length: 7 }, (_, i) => `/work/shot-${i + 1}.png`),
    highlighted: "/work/shot-5.png",
  });
  expect(await screen.findByRole("button", { name: "Enlarge shot-5.png" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Show less" })).toBeInTheDocument();
});

test("a deleted file loses its actions", async () => {
  mockFileInfo({ "/work/gone.txt": null });
  render(SharedFilesHarness, { paths: work("gone.txt") });
  expect(await screen.findByRole("alert")).toHaveTextContent("File no longer exists");
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
});
