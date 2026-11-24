import { flushSync } from "svelte";
import { expect, test } from "vitest";
import { createLocalHost } from "../src/modes/local/host.svelte";
import { DeviceRuntime } from "../src/device/runtime";
import { testDevice } from "./deviceFixture";

test("the local host follows the replacement runtime after a credential change", () => {
  const next = new DeviceRuntime("local", testDevice.access);
  let runtime = $state.raw(testDevice);
  const host = createLocalHost(() => runtime);
  expect(host.canSwitchDevice).toBe(false);
  expect(host.devices).toHaveLength(1);
  expect(host.activeDevice?.runtime).toBe(testDevice);
  runtime = next;
  next.connection.sseStatus.set("open");
  flushSync();
  expect(host.devices[0].runtime).toBe(next);
  expect(host.activeDevice?.state).toBe("online");
  next.dispose();
});
