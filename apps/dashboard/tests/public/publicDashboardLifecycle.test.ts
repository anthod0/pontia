import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import PublicDashboardHarness from "../components/PublicDashboardHarness.svelte";
import PublicDashboardRouteHarness from "../components/PublicDashboardRouteHarness.svelte";

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

afterEach(() => {
  vi.unstubAllGlobals();
});

test("retries a failed device list request", async () => {
  const fetchMock = vi
    .fn()
    .mockResolvedValueOnce(new Response(null, { status: 503 }))
    .mockResolvedValueOnce(
      new Response(JSON.stringify([{ device_handle: "office-mac", name: "Office Mac" }]), {
        headers: { "Content-Type": "application/json" },
      }),
    );
  vi.stubGlobal("fetch", fetchMock);

  render(PublicDashboardHarness, { props: { handle: "" } });
  await fireEvent.click(await screen.findByRole("button", { name: "Try again" }));

  const openDashboard = await screen.findByRole("link", { name: "Open Dashboard" });
  expect(openDashboard).toHaveAttribute("href", "https://app.pontia.dev/office-mac");
  expect(fetchMock).toHaveBeenCalledTimes(2);
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

test("navigation within the same device preserves the secure connection and mounted dashboard", async () => {
  const view = render(PublicDashboardRouteHarness, {
    props: { data: { handle: "office-mac" } },
  });
  const draft = await screen.findByRole("textbox", { name: "Draft" });
  await fireEvent.input(draft, { target: { value: "Unsent message" } });
  mocks.stopRuntime.mockClear();
  mocks.clearRuntime.mockClear();

  await view.rerender({ data: { handle: "office-mac" } });

  expect(mocks.resolveTarget).toHaveBeenCalledTimes(1);
  expect(mocks.stopRuntime).not.toHaveBeenCalled();
  expect(mocks.clearRuntime).not.toHaveBeenCalled();
  expect(screen.getByRole("textbox", { name: "Draft" })).toBe(draft);
  expect(draft).toHaveValue("Unsent message");
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
