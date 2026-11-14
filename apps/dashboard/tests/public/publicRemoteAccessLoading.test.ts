import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import PublicDashboardHarness from "../components/PublicDashboardHarness.svelte";

const mocks = vi.hoisted(() => ({
  failNextLoad: true,
  startRuntime: vi.fn(),
}));

// Simulates the separately deployed remote access module failing to load once.
vi.mock("../../src/modes/public/remote-access", async (importOriginal) => {
  if (mocks.failNextLoad) {
    mocks.failNextLoad = false;
    throw new Error("Failed to fetch dynamically imported module");
  }
  return {
    ...(await importOriginal<typeof import("../../src/modes/public/remote-access")>()),
    listPublicDevices: async () => ({
      userId: "user-one",
      devices: [
        { handle: "office-mac", deviceId: "office-mac", edgeId: "edge-one", name: "Office Mac" },
      ],
    }),
    createRemoteConnection: ({ handle }: { handle: string; deviceId: string; edgeId: string }) => {
      const controller = new AbortController();
      return {
        e2eFetch: async () =>
          new Response(
            JSON.stringify({ data: { workspaces: [], active: [], pinned: [], list: [] } }),
          ),
        connect: async () => ({
          handle,
          deviceId: "01234567-89ab-cdef-0123-456789abcdef",
          edgeApiOrigin: "https://brave-silver-atlas.edge.pontia.dev",
        }),
        uploadStorageScope: async () => handle,
        signal: controller.signal,
        dispose: () => controller.abort(),
      };
    },
  };
});

vi.mock("../../src/services/dashboardRuntime", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    startDashboardRuntime: mocks.startRuntime,
    stopDashboardRuntime: vi.fn(),
    clearDashboardRuntimeState: vi.fn(),
  };
  return {
    ...original,
    ...mocked,
    createLifecycle: (device) => ({
      ...original.createLifecycle(device),
      ...mocked,
      startDashboardRuntime() {
        mocks.startRuntime();
        device.connection.sseStatus.set("open");
      },
    }),
  };
});

describe("remote access loading", () => {
  test("a failed module load does not start the dashboard and can be retried", async () => {
    render(PublicDashboardHarness, { props: { handle: "office-mac" } });

    const retry = await screen.findByRole("button", { name: "Try again" });
    expect(mocks.startRuntime).not.toHaveBeenCalled();
    expect(screen.queryByText("Dashboard content")).not.toBeInTheDocument();

    await fireEvent.click(retry);
    await waitFor(() => expect(screen.getByText("Dashboard content")).toBeInTheDocument());
    expect(mocks.startRuntime).toHaveBeenCalledTimes(1);
  });
});
