import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SharedFilesHarness from "./SharedFilesHarness.svelte";
import { downloads } from "../../src/lib/downloads/manager";
import { testDevice } from "../deviceFixture";
import { ApiError } from "../../src/api/errors";
import { VisibleObserver, missing, mockFileInfo } from "./fileCardFixture";

const name = "release-bundle-v1.8.0.zip";
const path = `/work/${name}`;
const size = 3 * 1024 * 1024;
let unregister: () => void;

beforeEach(() => {
  unregister = downloads.registerDevice(testDevice);
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:bot-file");
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  mockFileInfo({ [path]: { size } });
});

afterEach(() => {
  for (const task of get(downloads)) downloads.cancel(task.id);
  unregister();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

test("shows reception progress with a cancel button while downloading", async () => {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  vi.spyOn(testDevice.fileApi, "pathFileContent").mockResolvedValue(
    new Response(new ReadableStream({ start: (controller) => void (stream = controller) }), {
      headers: { "Content-Length": String(size) },
    }),
  );
  render(SharedFilesHarness, { paths: [path] });
  expect(await screen.findByText("ZIP · 3 MiB")).toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: `Download ${name}` }));
  stream.enqueue(new Uint8Array(1024 * 1024));
  await waitFor(() =>
    expect(get(downloads)[0].progress).toEqual({ received: 1024 * 1024, total: size }),
  );
  expect(screen.getByRole("status")).toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: `Cancel download of ${name}` }));
  await waitFor(() => expect(get(downloads)).toEqual([]));
  expect(screen.getByRole("button", { name: `Download ${name}` })).toBeInTheDocument();
});

test("retries a failed download and saves it successfully", async () => {
  vi.spyOn(testDevice.fileApi, "pathFileContent")
    .mockRejectedValueOnce(new ApiError("Read failed", "internal_error", 500))
    .mockResolvedValueOnce(new Response("archive"));
  render(SharedFilesHarness, { paths: [path] });
  await fireEvent.click(screen.getByRole("button", { name: `Download ${name}` }));
  expect(await screen.findByRole("alert")).toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: `Retry download of ${name}` }));
  await waitFor(() => expect(HTMLAnchorElement.prototype.click).toHaveBeenCalledTimes(1));
  expect(get(downloads)).toEqual([]);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("marks a file deleted after it was shown as unavailable", async () => {
  vi.spyOn(testDevice.fileApi, "pathFileContent").mockRejectedValue(missing());
  render(SharedFilesHarness, { paths: [path] });
  await fireEvent.click(screen.getByRole("button", { name: `Download ${name}` }));
  expect(await screen.findByRole("alert")).toBeInTheDocument();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
});

test("falls back to the file bubble when an attached image is gone", async () => {
  mockFileInfo({ "/work/gone.png": null });
  render(SharedFilesHarness, { paths: ["/work/gone.png"] });
  expect(await screen.findByRole("alert")).toBeInTheDocument();
  expect(screen.queryByRole("img", { name: "gone.png" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
});

test("shows an attached image inline", async () => {
  vi.spyOn(testDevice.previews, "imageUrl").mockResolvedValue("blob:chart");
  render(SharedFilesHarness, { paths: ["/work/chart.png"] });
  expect(await screen.findByRole("img", { name: "chart.png" })).toHaveAttribute(
    "src",
    "blob:chart",
  );
});

test.each(["release-bundle-v1.8.0.zip", "demo.mp4", "paper.pdf"])(
  "%s offers only a download, without a preview",
  (file) => {
    render(SharedFilesHarness, { paths: [`/work/${file}`] });
    expect(screen.queryByRole("button", { name: `Preview ${file}` })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Download ${file}` })).toBeInTheDocument();
  },
);

test.each(["notes.md", "Makefile"])("%s opens in the preview panel", async (file) => {
  const read = vi
    .spyOn(testDevice.fileApi, "pathFileContent")
    .mockResolvedValue(new Response("plain text"));
  render(SharedFilesHarness, { paths: [`/work/${file}`] });
  await fireEvent.click(screen.getByRole("button", { name: `Preview ${file}` }));
  await within(screen.getByRole("complementary")).findByText("plain text");
  expect(read).toHaveBeenCalledWith("main", `/work/${file}`, expect.any(AbortSignal));
});
