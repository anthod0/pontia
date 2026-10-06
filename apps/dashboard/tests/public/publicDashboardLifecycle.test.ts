import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";
import PublicDashboardHarness from "../components/PublicDashboardHarness.svelte";

const mocks = vi.hoisted(() => ({
  resolveTarget: vi.fn(),
  startRuntime: vi.fn(),
  stopRuntime: vi.fn(),
  clearRuntime: vi.fn(),
}));

vi.mock("../../src/modes/public/e2eTransport", () => ({
  connectPublicDevice: mocks.resolveTarget,
  clearE2eSession: vi.fn(),
}));

vi.mock("../../src/services/dashboardRuntime", () => ({
  startDashboardRuntime: mocks.startRuntime,
  stopDashboardRuntime: mocks.stopRuntime,
  clearDashboardRuntimeState: mocks.clearRuntime,
}));

const target = (handle: string) => ({
  handle,
  deviceId:
    handle === "office-mac"
      ? "01234567-89ab-cdef-0123-456789abcdef"
      : "12345678-9abc-def0-1234-56789abcdef0",
  edgeApiOrigin: "https://brave-silver-atlas.edge.pontia.dev",
});

beforeEach(() => {
  vi.clearAllMocks();
  mocks.resolveTarget.mockImplementation(async (handle: string) => target(handle));
});

test("starts only after the secure connection completes and tears down before a scope switch", async () => {
  const view = render(PublicDashboardHarness, { props: { handle: "office-mac" } });

  expect(mocks.startRuntime).not.toHaveBeenCalled();
  expect(screen.queryByText("Dashboard content")).not.toBeInTheDocument();

  await waitFor(() => expect(screen.getByText("Dashboard content")).toBeInTheDocument());
  expect(mocks.startRuntime).toHaveBeenCalledTimes(1);

  let resolveNextTarget: ((value: ReturnType<typeof target>) => void) | undefined;
  mocks.resolveTarget.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveNextTarget = resolve;
      }),
  );
  await view.rerender({ handle: "travel-laptop" });
  expect(mocks.startRuntime).toHaveBeenCalledTimes(1);
  expect(screen.queryByText("Dashboard content")).not.toBeInTheDocument();
  expect(mocks.stopRuntime).toHaveBeenCalled();
  expect(mocks.clearRuntime).toHaveBeenCalled();

  resolveNextTarget?.(target("travel-laptop"));
  await waitFor(() => expect(screen.getByText("Dashboard content")).toBeInTheDocument());
  expect(mocks.resolveTarget).toHaveBeenLastCalledWith("travel-laptop", expect.any(AbortSignal));
  expect(mocks.startRuntime).toHaveBeenCalledTimes(2);
});

test("a failed secure connection does not start the dashboard and can be retried", async () => {
  mocks.resolveTarget.mockRejectedValueOnce(new Error("handshake failed"));
  render(PublicDashboardHarness, { props: { handle: "office-mac" } });
  const retry = await screen.findByRole("button", { name: "Try again" });
  expect(mocks.startRuntime).not.toHaveBeenCalled();
  expect(screen.queryByText("Dashboard content")).not.toBeInTheDocument();
  await fireEvent.click(retry);
  await waitFor(() => expect(screen.getByText("Dashboard content")).toBeInTheDocument());
  expect(mocks.startRuntime).toHaveBeenCalledTimes(1);
});

test("leaving a pending connection prevents a late result from starting the dashboard", async () => {
  let finish!: (value: ReturnType<typeof target>) => void;
  mocks.resolveTarget.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(PublicDashboardHarness, { props: { handle: "office-mac" } });
  await waitFor(() => expect(mocks.resolveTarget).toHaveBeenCalled());
  const signal = mocks.resolveTarget.mock.calls[0][1] as AbortSignal;
  view.unmount();
  expect(signal.aborted).toBe(true);
  finish(target("office-mac"));
  await Promise.resolve();
  expect(mocks.startRuntime).not.toHaveBeenCalled();
});
