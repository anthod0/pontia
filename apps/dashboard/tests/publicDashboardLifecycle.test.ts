import { render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";
import PublicDashboardHarness from "./components/PublicDashboardHarness.svelte";

const mocks = vi.hoisted(() => ({
  resolveTarget: vi.fn(),
  startRuntime: vi.fn(),
  stopRuntime: vi.fn(),
  clearRuntime: vi.fn(),
}));

vi.mock("../src/modes/public/remoteAccess", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../src/modes/public/remoteAccess")>()),
  resolvePublicDeviceTarget: mocks.resolveTarget,
}));

vi.mock("../src/services/dashboardRuntime", () => ({
  startDashboardRuntime: mocks.startRuntime,
  stopDashboardRuntime: mocks.stopRuntime,
  clearDashboardRuntimeState: mocks.clearRuntime,
}));

const publicMode = import.meta.env.VITE_DASHBOARD_MODE === "public";
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

test.skipIf(!publicMode)(
  "starts only after target resolution and tears down before a scope switch",
  async () => {
    const view = render(PublicDashboardHarness, { props: { handle: "office-mac" } });

    expect(screen.getByRole("heading", { name: "Connecting to device" })).toBeInTheDocument();
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
    expect(screen.getByRole("heading", { name: "Connecting to device" })).toBeInTheDocument();
    expect(screen.queryByText("Dashboard content")).not.toBeInTheDocument();
    expect(mocks.stopRuntime).toHaveBeenCalled();
    expect(mocks.clearRuntime).toHaveBeenCalled();

    resolveNextTarget?.(target("travel-laptop"));
    await waitFor(() => expect(screen.getByText("Dashboard content")).toBeInTheDocument());
    expect(mocks.resolveTarget).toHaveBeenLastCalledWith("travel-laptop", expect.any(AbortSignal));
    expect(mocks.startRuntime).toHaveBeenCalledTimes(2);
  },
);
