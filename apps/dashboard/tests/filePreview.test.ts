import { testDevice } from "./deviceFixture";
import { Blob } from "node:buffer";
import { beforeEach, afterEach, expect, test, vi } from "vitest";
const { sharedImageUrl } = testDevice.previews;
const { sharedFileContent } = testDevice.fileApi;
testDevice.access.uploadStorageScope = async () => "test-device";
vi.mock("../src/api/sessionFiles", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    sharedFileContent: vi.fn(),
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
    vi.mocked(sharedFileContent).mockResolvedValueOnce(
      new Response("<svg/>", { headers: { "Content-Type": type } }),
    );
    expect(await sharedImageUrl("s", type)).toBeNull();
  }
});

test("preview rejects files that grew beyond the preview limit", async () => {
  vi.mocked(sharedFileContent).mockResolvedValueOnce(
    new Response("", {
      headers: { "Content-Type": "image/png", "Content-Length": String(20 * 1024 * 1024 + 1) },
    }),
  );
  expect(await sharedImageUrl("s", "grew")).toBeNull();
});

test("preview caches one object URL per shared image for the page lifetime", async () => {
  vi.mocked(sharedFileContent).mockResolvedValueOnce(
    new Response("png-bytes", { headers: { "Content-Type": "image/png" } }),
  );
  const first = await sharedImageUrl("s", "cached");
  const second = await sharedImageUrl("s", "cached");
  expect(first).toBe("blob:preview");
  expect(second).toBe(first);
  expect(await vi.mocked(URL.createObjectURL).mock.calls[0][0].text()).toBe("png-bytes");
  expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
  expect(sharedFileContent).toHaveBeenCalledTimes(1);
});
