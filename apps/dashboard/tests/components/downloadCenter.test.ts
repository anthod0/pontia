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
  downloads.clearFinished();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
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
  expect(get(downloads)[0].status).toBe("cancelled");
  expect(saved).not.toHaveBeenCalled();
});

test("a failed download can be retried and cleared from the dialog after leaving its card", async () => {
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ error: { code: "not_found", message: "missing" } }), {
          status: 404,
        }),
      )
      .mockResolvedValueOnce(new Response("report", { headers: { "Content-Length": "6" } })),
  );
  const card = render(SharedFileCard, { file, sessionId: "session" });
  render(DownloadCenter);
  await fireEvent.click(screen.getByRole("button", { name: "Download report.txt" }));
  card.unmount();
  await fireEvent.click(screen.getByRole("button", { name: "Downloads", exact: true }));
  await fireEvent.click(
    await screen.findByRole("button", { name: "Retry download of report.txt" }),
  );
  await waitFor(() => expect(saved).toHaveBeenCalledTimes(1));
  expect(get(downloads)[0].status).toBe("complete");
  await fireEvent.click(screen.getByRole("button", { name: "Clear finished" }));
  expect(get(downloads)).toHaveLength(0);
});
