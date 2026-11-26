import { testDevice } from "./deviceFixture";
import { Blob } from "node:buffer";
import { beforeEach, afterEach, expect, test, vi } from "vitest";
const { imageUrl } = testDevice.previews;
const { pathFileContent } = testDevice.fileApi;
testDevice.access.uploadStorageScope = async () => "test-device";
vi.mock("../src/api/sessionFiles", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    pathFileContent: vi.fn(),
  };
  return {
    ...original,
    ...mocked,
    createFileApi: (device) => ({ ...original.createFileApi(device), ...mocked }),
  };
});
beforeEach(() => {
  vi.stubGlobal("Blob", Blob);
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:preview");
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
  vi.restoreAllMocks();
});

test("preview trusts the response type and refuses active content", async () => {
  for (const type of ["image/svg+xml", "text/html", "application/octet-stream"]) {
    vi.mocked(pathFileContent).mockResolvedValueOnce(
      new Response("<svg/>", { headers: { "Content-Type": type } }),
    );
    expect(await imageUrl("s", `/work/${type}`)).toBeNull();
  }
});

test("preview rejects files that grew beyond the preview limit", async () => {
  vi.mocked(pathFileContent).mockResolvedValueOnce(
    new Response("", {
      headers: { "Content-Type": "image/png", "Content-Length": String(5 * 1024 * 1024 + 1) },
    }),
  );
  expect(await imageUrl("s", "/work/grew.png")).toBeNull();
});

test("preview accepts an image at the five MiB preview limit", async () => {
  vi.mocked(pathFileContent).mockResolvedValueOnce(
    new Response(new Uint8Array(5 * 1024 * 1024), {
      headers: { "Content-Type": "image/png" },
    }),
  );
  expect(await imageUrl("s", "/work/five-mib.png")).toBe("blob:preview");
  expect(vi.mocked(URL.createObjectURL).mock.calls[0][0].size).toBe(5 * 1024 * 1024);
});

test("preview caches one object URL per attached image for the page lifetime", async () => {
  vi.mocked(pathFileContent).mockResolvedValueOnce(
    new Response("png-bytes", { headers: { "Content-Type": "image/png" } }),
  );
  const first = await imageUrl("s", "/work/cached.png");
  const second = await imageUrl("s", "/work/cached.png");
  expect(first).toBe("blob:preview");
  expect(second).toBe(first);
  expect(await vi.mocked(URL.createObjectURL).mock.calls[0][0].text()).toBe("png-bytes");
  expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
  expect(pathFileContent).toHaveBeenCalledTimes(1);
});
