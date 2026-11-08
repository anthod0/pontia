import { testDevice } from "./deviceFixture";
import { render } from "@testing-library/svelte";
import { tick } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import ConnectionNotice from "../src/components/layout/ConnectionNotice.svelte";
const { sseStatus } = testDevice.connection;

const toast = vi.hoisted(() => ({ warning: vi.fn(), dismiss: vi.fn() }));
vi.mock("svelte-sonner", () => ({ toast }));

beforeEach(() => {
  vi.useFakeTimers();
  toast.warning.mockClear();
  toast.dismiss.mockClear();
});

afterEach(() => {
  sseStatus.set("idle");
  vi.useRealTimers();
});

test("shows a persistent toast only after the stream stays unhealthy", async () => {
  sseStatus.set("open");
  render(ConnectionNotice);

  sseStatus.set("reconnecting");
  await vi.advanceTimersByTimeAsync(2000);
  sseStatus.set("error");
  await vi.advanceTimersByTimeAsync(999);
  expect(toast.warning).not.toHaveBeenCalled();

  await vi.advanceTimersByTimeAsync(1);
  expect(toast.warning).toHaveBeenCalledOnce();
  expect(toast.warning).toHaveBeenCalledWith(
    "Connection lost, reconnecting…",
    expect.objectContaining({ id: "connection-lost", duration: Number.POSITIVE_INFINITY }),
  );

  sseStatus.set("open");
  await tick();
  expect(toast.dismiss).toHaveBeenCalledWith("connection-lost");
});

test("does not toast for a brief reconnect", async () => {
  sseStatus.set("open");
  render(ConnectionNotice);

  sseStatus.set("reconnecting");
  await vi.advanceTimersByTimeAsync(1500);
  sseStatus.set("open");
  await vi.advanceTimersByTimeAsync(5000);

  expect(toast.warning).not.toHaveBeenCalled();
});
