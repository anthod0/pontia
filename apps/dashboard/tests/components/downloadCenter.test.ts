import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import DownloadCenter from "../../src/components/layout/DownloadCenter.svelte";
import SharedFileCard from "../../src/components/chat/SharedFileCard.svelte";
import { downloads } from "../../src/lib/downloads/manager";

const file = {
  file_id: "file-1",
  turn_id: null,
  name: "report.txt",
  path: "/tmp/report.txt",
  size: 10,
  media_type: "text/plain",
  created_at: "2026-11-17T00:00:00Z",
};
let saved: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:download-center");
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  saved = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
});

afterEach(() => {
  for (const task of get(downloads)) downloads.cancel(task.id);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

test("the download button appears after a download starts and counts every listed download", async () => {
  render(DownloadCenter);
  expect(screen.queryByRole("button", { name: "Downloads", exact: true })).not.toBeInTheDocument();
  const first = downloads.start(
    "missing-device",
    "session",
    { kind: "shared", fileId: "file-1" },
    "report.txt",
  );
  expect(await screen.findByRole("button", { name: "Downloads", exact: true })).toBeInTheDocument();
  expect(screen.getByTestId("downloads-count")).toHaveTextContent("1");
  const second = downloads.start(
    "missing-device",
    "session",
    { kind: "shared", fileId: "file-2" },
    "notes.txt",
  );
  await waitFor(() => expect(screen.getByTestId("downloads-count")).toHaveTextContent("2"));
  downloads.cancel(first.id);
  downloads.cancel(second.id);
  await waitFor(() =>
    expect(
      screen.queryByRole("button", { name: "Downloads", exact: true }),
    ).not.toBeInTheDocument(),
  );
});

test("a shared file keeps receiving after its card and download dialog close, and can be cancelled centrally", async () => {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  const cancelled = vi.fn();
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      stream = controller;
    },
    cancel: cancelled,
  });
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(new Response(body, { headers: { "Content-Length": "10" } })),
  );
  const card = render(SharedFileCard, { file, sessionId: "session" });
  render(DownloadCenter);
  await fireEvent.click(screen.getByRole("button", { name: "Download report.txt" }));
  stream.enqueue(new TextEncoder().encode("first"));
  card.unmount();
  await fireEvent.click(screen.getByRole("button", { name: "Downloads", exact: true }));
  const dialog = await screen.findByRole("dialog");
  await waitFor(() =>
    expect(within(dialog).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "50"),
  );
  await fireEvent.click(within(dialog).getByRole("button", { name: "Close", exact: true }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  stream.enqueue(new TextEncoder().encode("more"));
  await waitFor(() => expect(get(downloads)[0].progress.received).toBe(9));
  await fireEvent.click(screen.getByRole("button", { name: "Downloads", exact: true }));
  await fireEvent.click(
    await screen.findByRole("button", { name: "Cancel download of report.txt" }),
  );
  await waitFor(() => expect(cancelled).toHaveBeenCalled());
  expect(get(downloads)).toEqual([]);
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(saved).not.toHaveBeenCalled();
});

test("a failed download stays in the dialog after leaving its card and leaves it once retried successfully", async () => {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ error: { code: "not_found", message: "missing" } }), {
          status: 404,
        }),
      )
      .mockResolvedValueOnce(
        new Response(
          new ReadableStream<Uint8Array>({
            start(controller) {
              stream = controller;
            },
          }),
          { headers: { "Content-Length": "6" } },
        ),
      ),
  );
  const card = render(SharedFileCard, { file, sessionId: "session" });
  render(DownloadCenter);
  await fireEvent.click(screen.getByRole("button", { name: "Download report.txt" }));
  card.unmount();
  await fireEvent.click(screen.getByRole("button", { name: "Downloads", exact: true }));
  const dialog = await screen.findByRole("dialog");
  expect(await within(dialog).findByText("File no longer exists")).toBeInTheDocument();
  await fireEvent.click(
    within(dialog).getByRole("button", { name: "Retry download of report.txt" }),
  );
  await waitFor(() => expect(within(dialog).getByRole("progressbar")).toBeInTheDocument());
  stream.enqueue(new TextEncoder().encode("report"));
  stream.close();
  await waitFor(() => expect(saved).toHaveBeenCalledTimes(1));
  expect(get(downloads)).toEqual([]);
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(screen.queryByRole("button", { name: "Downloads", exact: true })).not.toBeInTheDocument();
});

test("a failed download can be dismissed from the dialog", async () => {
  vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("offline")));
  render(SharedFileCard, { file, sessionId: "session" });
  render(DownloadCenter);
  await fireEvent.click(screen.getByRole("button", { name: "Download report.txt" }));
  await fireEvent.click(await screen.findByRole("button", { name: "Downloads", exact: true }));
  const dialog = await screen.findByRole("dialog");
  expect(await within(dialog).findByText("offline")).toBeInTheDocument();
  await fireEvent.click(
    within(dialog).getByRole("button", { name: "Dismiss failed download of report.txt" }),
  );
  expect(get(downloads)).toEqual([]);
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
});
