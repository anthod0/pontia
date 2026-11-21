import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { createDownloads } from "../src/lib/downloads/manager";
import { ApiError } from "../src/api/errors";

let manager: ReturnType<typeof createDownloads>;
let saved: Blob[];

function device(id: string) {
  return {
    id,
    name: id,
    fileApi: {
      sharedFileContent: vi.fn<(...args: [string, string, AbortSignal?]) => Promise<Response>>(),
      pathFileContent: vi.fn<(...args: [string, string, AbortSignal?]) => Promise<Response>>(),
      listSessionFiles: vi.fn(),
    },
  };
}

function streaming() {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  const cancel = vi.fn();
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      stream = controller;
    },
    cancel,
  });
  return { stream, cancel, response: new Response(body, { headers: { "Content-Length": "10" } }) };
}

beforeEach(() => {
  manager = createDownloads();
  saved = [];
  vi.spyOn(URL, "createObjectURL").mockImplementation((blob) => {
    saved.push(blob as Blob);
    return "blob:test-download";
  });
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
});

afterEach(() => {
  for (const task of get(manager)) manager.cancel(task.id);
  vi.restoreAllMocks();
});

test("deduplicates an active file and cancels reception without exporting partial bytes", async () => {
  const source = device("studio");
  const transfer = streaming();
  source.fileApi.pathFileContent.mockResolvedValue(transfer.response);
  manager.registerDevice(source);
  const task = manager.start(
    "studio",
    "session",
    { kind: "path", path: "/report.txt" },
    "report.txt",
  );
  transfer.stream.enqueue(new TextEncoder().encode("first"));
  await vi.waitFor(() => expect(get(manager)[0].progress.received).toBe(5));
  manager.start("studio", "session", { kind: "path", path: "/report.txt" }, "report.txt");
  expect(get(manager)).toHaveLength(1);
  expect(source.fileApi.pathFileContent).toHaveBeenCalledTimes(1);
  manager.cancel(task.id);
  await vi.waitFor(() => expect(transfer.cancel).toHaveBeenCalled());
  expect(get(manager)).toEqual([]);
  expect(source.fileApi.pathFileContent.mock.calls[0][2]?.aborted).toBe(true);
  expect(saved).toHaveLength(0);
});

test("a disconnected download retries from the start on its original device's new connection", async () => {
  const original = device("studio");
  const other = device("laptop");
  const transfer = streaming();
  original.fileApi.sharedFileContent.mockResolvedValue(transfer.response);
  const disconnect = manager.registerDevice(original);
  manager.registerDevice(other);
  const task = manager.start(
    "studio",
    "session",
    { kind: "shared", fileId: "file-1" },
    "report.txt",
  );
  transfer.stream.enqueue(new TextEncoder().encode("first"));
  await vi.waitFor(() => expect(get(manager)[0].progress.received).toBe(5));
  disconnect();
  await vi.waitFor(() => expect(get(manager)[0].status).toBe("failed"));
  expect(saved).toHaveLength(0);

  const reconnected = device("studio");
  reconnected.fileApi.sharedFileContent.mockResolvedValue(
    new Response("replacement", { headers: { "Content-Length": "11" } }),
  );
  manager.registerDevice(reconnected);
  manager.retry(task.id);
  await vi.waitFor(() => expect(saved).toHaveLength(1));
  expect(await saved[0].text()).toBe("replacement");
  expect(get(manager)).toEqual([]);
  expect(reconnected.fileApi.sharedFileContent.mock.calls[0].slice(0, 2)).toEqual([
    "session",
    "file-1",
  ]);
  expect(other.fileApi.sharedFileContent).not.toHaveBeenCalled();
});

test("a finished download leaves the list while another device keeps receiving", async () => {
  const source = device("studio");
  const other = device("laptop");
  source.fileApi.pathFileContent.mockResolvedValue(
    new Response("first file", { headers: { "Content-Length": "10" } }),
  );
  const transfer = streaming();
  other.fileApi.pathFileContent.mockResolvedValue(transfer.response);
  manager.registerDevice(source);
  manager.registerDevice(other);
  manager.start("studio", "session", { kind: "path", path: "/report.txt" }, "report.txt");
  const active = manager.start(
    "laptop",
    "session",
    { kind: "path", path: "/report.txt" },
    "report.txt",
  );
  await vi.waitFor(() => expect(saved).toHaveLength(1));
  expect(get(manager).map((task) => task.id)).toEqual([active.id]);
  transfer.stream.enqueue(new TextEncoder().encode("other file"));
  transfer.stream.close();
  await vi.waitFor(() => expect(get(manager)).toEqual([]));
  expect(await saved[1].text()).toBe("other file");
});

test("a failed download stays listed until it is dismissed", async () => {
  const source = device("studio");
  source.fileApi.sharedFileContent.mockRejectedValue(new Error("offline"));
  manager.registerDevice(source);
  const task = manager.start(
    "studio",
    "session",
    { kind: "shared", fileId: "file-1" },
    "report.txt",
  );
  await vi.waitFor(() =>
    expect(get(manager)[0]).toMatchObject({ status: "failed", error: "offline" }),
  );
  manager.cancel(task.id);
  expect(get(manager)).toEqual([]);
});

test("a failed download keeps the API error code and clears it on retry", async () => {
  const source = device("studio");
  source.fileApi.sharedFileContent
    .mockRejectedValueOnce(new ApiError("File not found", "not_found", 404))
    .mockResolvedValueOnce(new Response("report", { headers: { "Content-Length": "6" } }));
  manager.registerDevice(source);
  const task = manager.start(
    "studio",
    "session",
    { kind: "shared", fileId: "file-1" },
    "report.txt",
  );
  await vi.waitFor(() => expect(get(manager)[0].status).toBe("failed"));
  expect(get(manager)[0]).toMatchObject({ errorCode: "not_found", error: "File no longer exists" });
  manager.retry(task.id);
  expect(get(manager)[0]).toMatchObject({ status: "receiving", errorCode: null, error: null });
  await vi.waitFor(() => expect(get(manager)).toEqual([]));
});
