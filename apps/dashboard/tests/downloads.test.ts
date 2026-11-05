import { Blob } from "node:buffer";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { cleanupDownloads, saveDownload, type DownloadProgress } from "../src/lib/downloads/save";

class Directory {
  directories = new Map<string, Directory>();
  files = new Map<string, Blob>();
  async getDirectoryHandle(name: string, options?: { create?: boolean }) {
    if (!this.directories.has(name)) {
      if (!options?.create) throw new DOMException("missing", "NotFoundError");
      this.directories.set(name, new Directory());
    }
    return this.directories.get(name)!;
  }
  async getFileHandle(name: string, options?: { create?: boolean }) {
    if (!this.files.has(name)) {
      if (!options?.create) throw new DOMException("missing", "NotFoundError");
      this.files.set(name, new Blob());
    }
    return {
      getFile: async () => this.files.get(name)!,
      createWritable: async () => {
        const chunks: any[] = [];
        return {
          write: async (value: any) => {
            chunks.push(value);
          },
          close: async () => {
            this.files.set(name, new Blob(chunks));
          },
          abort: async () => {},
        };
      },
    };
  }
  async removeEntry(name: string) {
    this.directories.delete(name);
  }
  async *keys() {
    yield* this.directories.keys();
  }
}
let exported: Blob[];
let root: Directory;
let held: Set<string>;
function response(size: number, known = true) {
  let remaining = size;
  return new Response(
    new ReadableStream({
      pull(controller) {
        if (!remaining) {
          controller.close();
          return;
        }
        const count = Math.min(remaining, 1024 * 1024);
        controller.enqueue(new Uint8Array(count).fill(42));
        remaining -= count;
      },
    }),
    { headers: known ? { "Content-Length": String(size) } : {} },
  );
}
beforeEach(() => {
  vi.useFakeTimers();
  exported = [];
  root = new Directory();
  held = new Set();
  vi.stubGlobal("Blob", Blob);
  vi.spyOn(URL, "createObjectURL").mockImplementation((blob) => {
    exported.push(blob as unknown as Blob);
    return "blob:test";
  });
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  vi.stubGlobal("navigator", {
    storage: { getDirectory: async () => root },
    locks: {
      request: async (name: string, options: any, callback?: any) => {
        const run = callback ?? options;
        if (held.has(name)) return run(null);
        held.add(name);
        try {
          return await run({ name });
        } finally {
          held.delete(name);
        }
      },
    },
  });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

test("saves 64 MiB in memory and larger responses through temporary storage with reception progress", async () => {
  await saveDownload(response(64 * 1024 * 1024), "small", () => {});
  expect(root.directories.size).toBe(0);
  expect(exported[0].size).toBe(64 * 1024 * 1024);
  const progress: DownloadProgress[] = [];
  await saveDownload(response(65 * 1024 * 1024), "large", (value) => progress.push(value));
  expect(exported[1].size).toBe(65 * 1024 * 1024);
  const directory = root.directories.get("pontia-downloads")!;
  expect(directory.directories.size).toBe(1);
  expect(progress[0]).toEqual({ received: 0, total: 65 * 1024 * 1024 });
  expect(progress.at(-1)).toEqual({ received: 65 * 1024 * 1024, total: 65 * 1024 * 1024 });
  await vi.advanceTimersByTimeAsync(59_999);
  await cleanupDownloads();
  expect(directory.directories.size).toBe(1);
  await vi.advanceTimersByTimeAsync(1);
  expect(directory.directories.size).toBe(0);
});

test("unknown size uses temporary storage and indeterminate progress", async () => {
  const progress: DownloadProgress[] = [];
  await saveDownload(response(3, false), "unknown", (value) => progress.push(value));
  expect(root.directories.get("pontia-downloads")!.directories.size).toBe(1);
  expect(progress.at(-1)).toEqual({ received: 3, total: null });
  expect([...new Uint8Array(await exported[0].arrayBuffer())]).toEqual([42, 42, 42]);
});

test("unavailable OPFS falls back to a complete in-memory download", async () => {
  vi.stubGlobal("navigator", { storage: {} });
  await saveDownload(response(65 * 1024 * 1024), "fallback", () => {});
  expect(exported[0].size).toBe(65 * 1024 * 1024);
});

test("page-open cleanup respects retained files and another tab's active download", async () => {
  const directory = await root.getDirectoryHandle("pontia-downloads", { create: true });
  for (const [id, expiry] of [
    ["expired", 0],
    ["receiving", 0],
    ["retained", Date.now() + 102_400],
  ] as const) {
    const dir = await directory.getDirectoryHandle(id, { create: true });
    dir.files.set("expires", new Blob([String(expiry)]));
  }
  held.add("pontia-download:receiving");
  await cleanupDownloads();
  expect([...directory.directories.keys()]).toEqual(["receiving", "retained"]);
  held.clear();
  await vi.advanceTimersByTimeAsync(102_400);
  await cleanupDownloads();
  expect(directory.directories.size).toBe(0);
});
