import { afterEach, expect, test, vi } from "vitest";
import { listWorkspaces } from "../../src/api/client";
import { clearPublicApiTarget, setPublicApiTarget } from "../../src/modes/public/apiTarget";

afterEach(() => {
  clearPublicApiTarget();
  vi.unstubAllGlobals();
});

test("routes ordinary API requests through the trusted edge target", async () => {
  setPublicApiTarget({
    handle: "office-mac",
    deviceId: "01234567-89ab-cdef-0123-456789abcdef",
    edgeApiOrigin: "https://brave-silver-atlas.edge.pontia.dev",
  });
  const fetchMock = vi.fn(
    async () => new Response(JSON.stringify({ data: { workspaces: [] } }), { status: 200 }),
  );
  vi.stubGlobal("fetch", fetchMock);

  await expect(listWorkspaces()).resolves.toEqual([]);

  const [url, init] = fetchMock.mock.calls[0];
  expect(url).toBe(
    "https://brave-silver-atlas.edge.pontia.dev/devices/01234567-89ab-cdef-0123-456789abcdef/api/v1/workspaces",
  );
  expect(init.credentials).toBe("include");
  expect((init.headers as Headers).has("Authorization")).toBe(false);
  expect(init.signal).toBeInstanceOf(AbortSignal);

  clearPublicApiTarget();
  expect(init.signal?.aborted).toBe(true);
});
