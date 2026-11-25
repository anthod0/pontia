import { render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { DeviceRuntime } from "../src/device/runtime";
import { createLocalHost } from "../src/modes/local/host.svelte";
import { testDevice } from "./deviceFixture";
import MobileConnectionBannerHarness from "./components/MobileConnectionBannerHarness.svelte";

let runtime: DeviceRuntime;
beforeEach(() => {
  vi.useFakeTimers();
  runtime = new DeviceRuntime(crypto.randomUUID(), testDevice.access);
});
afterEach(() => {
  runtime.dispose();
  vi.useRealTimers();
});

test("the first connection after opening the page is not reported as lost", async () => {
  runtime.connection.sseStatus.set("connecting");
  render(MobileConnectionBannerHarness, { host: createLocalHost(() => runtime) });
  await vi.advanceTimersByTimeAsync(10_000);
  expect(screen.queryByText("Connection lost, reconnecting…")).toBeNull();
});

test("a device that was online reports a lost connection after three seconds", async () => {
  runtime.connection.sseStatus.set("open");
  render(MobileConnectionBannerHarness, { host: createLocalHost(() => runtime) });
  await vi.advanceTimersByTimeAsync(0);
  runtime.connection.sseStatus.set("reconnecting");
  await vi.advanceTimersByTimeAsync(2999);
  expect(screen.queryByText("Connection lost, reconnecting…")).toBeNull();
  await vi.advanceTimersByTimeAsync(1);
  expect(screen.getByText("Connection lost, reconnecting…")).toBeInTheDocument();
});
