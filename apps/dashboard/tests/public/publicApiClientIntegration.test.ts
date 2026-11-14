import { expect, test, vi } from "vitest";
import { DeviceRuntime } from "../../src/device/runtime";
import { createApiAccess } from "../../src/modes/public/apiAccess";
import { createRemoteConnection } from "../../src/modes/public/remote-access";

test("routes ordinary API requests through its public E2E connection", async () => {
  const connection = createRemoteConnection(
    {
      handle: "office-mac",
      deviceId: "01234567-89ab-cdef-0123-456789abcdef",
      edgeId: "01234567-89ab-cdef-0123-456789abcdef",
    },
    "01234567-89ab-cdef-0123-456789abcdef",
  );
  const e2eFetch = vi
    .spyOn(connection, "e2eFetch")
    .mockResolvedValue(new Response(JSON.stringify({ data: { workspaces: [] } }), { status: 200 }));
  const device = new DeviceRuntime("office-mac", createApiAccess(connection));
  await expect(device.api.listWorkspaces()).resolves.toEqual([]);
  const [path, init] = e2eFetch.mock.calls[0];
  expect(path).toBe("/api/v1/workspaces");
  expect((init.headers as Headers).has("Authorization")).toBe(false);
  expect(init.signal).toBeInstanceOf(AbortSignal);
  device.dispose();
  expect(init.signal?.aborted).toBe(true);
});
