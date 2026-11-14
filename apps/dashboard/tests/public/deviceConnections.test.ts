import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { get } from "svelte/store";
import { createDeviceConnections } from "../../src/modes/public/deviceConnections";
import { createRemoteAccessFixture } from "./fixtures/remoteConnections";

let fixture: ReturnType<typeof createRemoteAccessFixture>;
let manager: ReturnType<typeof createDeviceConnections>;
const device = (handle = "office-mac") =>
  get(manager.devices).find((entry) => entry.handle === handle)!;
const settle = () => vi.advanceTimersByTimeAsync(0);

beforeEach(() => {
  vi.useFakeTimers();
  fixture = createRemoteAccessFixture();
  manager = createDeviceConnections(fixture.remote);
});
afterEach(() => {
  manager.dispose();
  vi.useRealTimers();
});

test("connects all devices concurrently and receives independent events", async () => {
  let finish!: () => void;
  fixture.connect.mockImplementationOnce(
    (handle) =>
      new Promise((resolve) => {
        finish = () => resolve(fixture.target(handle));
      }),
  );
  await manager.refreshDevices();
  await settle();
  expect(device().state).toBe("connecting");
  expect(device("travel-laptop").state).toBe("online");
  finish();
  await settle();
  const a = device().runtime!;
  const b = device("travel-laptop").runtime!;
  fixture.event("office-mac", "a-1");
  fixture.event("travel-laptop", "b-1");
  await settle();
  expect(get(a.connection.dashboardStreamCursor)).toBe("a-1");
  expect(get(b.connection.dashboardStreamCursor)).toBe("b-1");
  a.chatDraft.setChatDraft("keep this draft");
  manager.focus();
  await settle();
  expect(device().runtime).toBe(a);
  expect(get(a.chatDraft.chatDraft)).toBe("keep this draft");
  expect(fixture.connect).toHaveBeenCalledTimes(2);
});

test("a handshake alone is not online until its event stream opens", async () => {
  const original = fixture.request.getMockImplementation()!;
  let open!: () => void;
  fixture.request.mockImplementation((handle, path, init) =>
    path.includes("/events/stream") && handle === "office-mac"
      ? new Promise((resolve) => {
          open = async () => resolve(await original(handle, path, init));
        })
      : original(handle, path, init),
  );
  await manager.refreshDevices();
  await settle();
  expect(device().state).toBe("connecting");
  expect(device("travel-laptop").state).toBe("online");
  open();
  await settle();
  expect(device().state).toBe("online");
});

test("retries a failed device after 5, 10, 20, 40, 60, 60 seconds without disturbing its peer", async () => {
  fixture.connect.mockImplementation(async (handle) => {
    if (handle === "office-mac") throw new Error("503");
    return fixture.target(handle);
  });
  await manager.refreshDevices();
  await settle();
  const peer = device("travel-laptop").runtime;
  for (const delay of [5000, 10000, 20000, 40000, 60000, 60000]) {
    const count = fixture.connect.mock.calls.length;
    await vi.advanceTimersByTimeAsync(delay - 1);
    expect(fixture.connect).toHaveBeenCalledTimes(count);
    await vi.advanceTimersByTimeAsync(1);
    expect(fixture.connect).toHaveBeenCalledTimes(count + 1);
  }
  expect(device("travel-laptop").runtime).toBe(peer);
  expect(fixture.signals.get("travel-laptop")![0].aborted).toBe(false);
});

test.each(["focus", "manual"])(
  "%s retries immediately, and a successful stream resets backoff",
  async (action) => {
    fixture.connect.mockRejectedValueOnce(new Error("handshake failed"));
    await manager.refreshDevices();
    await settle();
    expect(device().state).toBe("offline");
    if (action === "focus") manager.focus();
    else manager.retry("office-mac");
    await settle();
    expect(device().state).toBe("online");
    fixture.streams.get("office-mac")!.close();
    await settle();
    expect(device().state).toBe("offline");
    const count = fixture.connect.mock.calls.length;
    await vi.advanceTimersByTimeAsync(4999);
    expect(fixture.connect).toHaveBeenCalledTimes(count);
    await vi.advanceTimersByTimeAsync(1);
    expect(device().state).toBe("online");
  },
);

test("refresh adds devices and cancels removed devices, including pending connections and retries", async () => {
  let finish!: () => void;
  fixture.connect
    .mockImplementationOnce(
      (handle) =>
        new Promise((resolve) => {
          finish = () => resolve(fixture.target(handle));
        }),
    )
    .mockRejectedValueOnce(new Error("offline"));
  await manager.refreshDevices();
  await settle();
  const oldSignals = [...fixture.signals.values()].flat();
  fixture.listed.splice(0, 2, {
    handle: "new-device",
    deviceId: "new-device",
    edgeId: "edge-one",
    name: "New device",
  });
  await manager.refreshDevices();
  finish();
  await settle();
  expect(oldSignals.every((signal) => signal.aborted)).toBe(true);
  expect(get(manager.devices).map(({ handle, state }) => [handle, state])).toEqual([
    ["new-device", "online"],
  ]);
  await vi.advanceTimersByTimeAsync(120000);
  expect(fixture.connect).toHaveBeenCalledTimes(3);
});

test.each(["list", "connect", "reauthorize"])(
  "central 401 during %s stops online, pending and offline devices until sign-in",
  async (source) => {
    fixture.listed.push(
      { handle: "pending-device", deviceId: "pending-device", edgeId: "edge-one", name: "Pending" },
      { handle: "offline-device", deviceId: "offline-device", edgeId: "edge-one", name: "Offline" },
    );
    let finish!: () => void;
    fixture.connect.mockImplementation(async (handle) => {
      if (handle === "pending-device")
        return new Promise((resolve) => {
          finish = () => resolve(fixture.target(handle));
        });
      if (handle === "offline-device") throw new Error("offline");
      return fixture.target(handle);
    });
    await manager.refreshDevices();
    await settle();
    const runtimes = get(manager.devices).flatMap(({ runtime }) => (runtime ? [runtime] : []));
    if (source === "list") {
      vi.mocked(fixture.remote.listPublicDevices).mockRejectedValueOnce(
        new fixture.remote.CloudRequestError("unauthorized", 401),
      );
      await manager.refreshDevices();
    } else if (source === "connect") {
      fixture.connect.mockRejectedValueOnce(
        new fixture.remote.CloudRequestError("unauthorized", 401),
      );
      manager.retry("offline-device");
    } else fixture.authorizationFailures.get("office-mac")!();
    await settle();
    expect(get(manager.signInRequired)).toBe(true);
    expect(fixture.remote.retainSavedConnections).toHaveBeenLastCalledWith(null, []);
    expect(get(manager.devices).every(({ state }) => state === "sign-in-required")).toBe(true);
    expect([...fixture.signals.values()].flat().every((signal) => signal.aborted)).toBe(true);
    expect(runtimes.every((runtime) => get(runtime.connection.sseStatus) !== "open")).toBe(true);
    finish();
    const attempts = fixture.connect.mock.calls.length;
    const lists = vi.mocked(fixture.remote.listPublicDevices).mock.calls.length;
    manager.focus();
    manager.retry("offline-device");
    await manager.refreshDevices();
    await vi.advanceTimersByTimeAsync(180000);
    expect(fixture.connect).toHaveBeenCalledTimes(attempts);
    expect(fixture.remote.listPublicDevices).toHaveBeenCalledTimes(lists);
    expect(get(manager.devices).every(({ runtime }) => runtime === null)).toBe(true);
  },
);

test("non-401 list failures preserve established connections and can be retried", async () => {
  await manager.refreshDevices();
  await settle();
  const runtime = device().runtime;
  vi.mocked(fixture.remote.listPublicDevices).mockRejectedValueOnce(
    new fixture.remote.CloudRequestError("unavailable", 503),
  );
  await manager.refreshDevices();
  expect(get(manager.error)).not.toBeNull();
  expect(device().runtime).toBe(runtime);
  await manager.refreshDevices();
  expect(get(manager.error)).toBeNull();
  expect(fixture.connect).toHaveBeenCalledTimes(2);
});

test("failed reauthorization during an ordinary request takes only its device offline and retries", async () => {
  await manager.refreshDevices();
  await settle();
  const peer = device("travel-laptop").runtime;
  expect(device().state).toBe("online");
  fixture.connectionFailures.get("office-mac")!(new Error("handshake failed"));
  expect(device().state).toBe("offline");
  expect(fixture.signals.get("office-mac")![0].aborted).toBe(true);
  expect(device("travel-laptop").runtime).toBe(peer);
  await vi.advanceTimersByTimeAsync(5000);
  expect(device().state).toBe("online");
  expect(fixture.signals.get("travel-laptop")).toHaveLength(1);
});

test("a user switch replaces active runtimes even when device handles stay the same", async () => {
  await manager.refreshDevices();
  await settle();
  const previous = device().runtime;
  const signals = [...fixture.signals.values()].flat();
  vi.mocked(fixture.remote.listPublicDevices).mockResolvedValueOnce({
    userId: "user-two",
    devices: [...fixture.listed],
  });
  await manager.refreshDevices();
  await settle();
  expect(signals.every((signal) => signal.aborted)).toBe(true);
  expect(device().runtime).not.toBe(previous);
  expect(device().state).toBe("online");
  expect(fixture.remote.retainSavedConnections).toHaveBeenLastCalledWith(
    "user-two",
    fixture.listed,
  );
});

test("reassigning a handle replaces the old device runtime", async () => {
  await manager.refreshDevices();
  await settle();
  const previous = device().runtime;
  const signal = fixture.signals.get("office-mac")![0];
  fixture.listed[0].deviceId = "replacement-device";
  await manager.refreshDevices();
  await settle();
  expect(signal.aborted).toBe(true);
  expect(device().runtime).not.toBe(previous);
  expect(device().deviceId).toBe("replacement-device");
  expect(device().state).toBe("online");
});

test("moving a device to another edge replaces its cached connection and runtime", async () => {
  await manager.refreshDevices();
  await settle();
  const previous = device().runtime;
  const signal = fixture.signals.get("office-mac")![0];
  fixture.listed[0].edgeId = "edge-two";
  await manager.refreshDevices();
  await settle();
  expect(signal.aborted).toBe(true);
  expect(device().runtime).not.toBe(previous);
  expect(device().edgeId).toBe("edge-two");
  expect(device().state).toBe("online");
});
