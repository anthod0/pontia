import { Blob } from "node:buffer";
import { beforeEach, afterEach, expect, test, vi } from "vitest";
import { filePreview } from "../src/lib/downloads/preview";
import { sharedFileContent } from "../src/api/sessionFiles";
vi.mock("$dashboard-mode/apiAccess", () => ({ uploadStorageScope: async () => "test-device" }));
vi.mock("../src/api/sessionFiles", () => ({ sharedFileContent: vi.fn() }));
beforeEach(() => {
  vi.stubGlobal("Blob", Blob);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

test("preview trusts the response type and refuses active content", async () => {
  for (const type of ["image/svg+xml", "text/html", "application/octet-stream"]) {
    vi.mocked(sharedFileContent).mockResolvedValueOnce(
      new Response("<svg/>", { headers: { "Content-Type": type } }),
    );
    expect(await filePreview("s", type)).toBeNull();
  }
});

test("preview rejects files that grew beyond the preview limit", async () => {
  vi.mocked(sharedFileContent).mockResolvedValueOnce(
    new Response("", {
      headers: { "Content-Type": "image/png", "Content-Length": String(20 * 1024 * 1024 + 1) },
    }),
  );
  expect(await filePreview("s", "grew")).toBeNull();
});

test("preview caches allowed image bytes for the page lifetime", async () => {
  vi.mocked(sharedFileContent).mockResolvedValueOnce(
    new Response("png-bytes", { headers: { "Content-Type": "image/png" } }),
  );
  const first = await filePreview("s", "cached");
  const second = await filePreview("s", "cached");
  expect(await first!.text()).toBe("png-bytes");
  expect(second).toBe(first);
  expect(sharedFileContent).toHaveBeenCalledTimes(1);
});
