import { afterEach, expect, test, vi } from "vitest";

const { e2eFetch } = vi.hoisted(() => ({
  e2eFetch: vi.fn(
    async () => new Response(JSON.stringify({ data: { workspaces: [] } }), { status: 200 }),
  ),
}));
vi.mock("../../src/modes/public/e2eTransport", () => ({
  e2eFetch,
  clearE2eSession: vi.fn(),
}));

import { listWorkspaces } from "../../src/api/client";
import { clearPublicApiTarget, setPublicApiTarget } from "../../src/modes/public/apiTarget";

afterEach(() => {
  clearPublicApiTarget();
  e2eFetch.mockClear();
});

test("routes ordinary API requests through the public E2E transport seam", async () => {
  setPublicApiTarget({
    handle: "office-mac",
    deviceId: "01234567-89ab-cdef-0123-456789abcdef",
    edgeApiOrigin: "https://brave-silver-atlas.edge.pontia.dev",
  });

  await expect(listWorkspaces()).resolves.toEqual([]);

  const [path, init] = e2eFetch.mock.calls[0];
  expect(path).toBe("/api/v1/workspaces");
  expect((init.headers as Headers).has("Authorization")).toBe(false);
  expect(init.signal).toBeInstanceOf(AbortSignal);

  clearPublicApiTarget();
  expect(init.signal?.aborted).toBe(true);
});
