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
    connectPublicDevice: async (handle: string) => ({
      handle,
      deviceId: "01234567-89ab-cdef-0123-456789abcdef",
      edgeApiOrigin: "https://brave-silver-atlas.edge.pontia.dev",
    }),
    clearE2eSession: vi.fn(),
  };
});

vi.mock("../../src/services/dashboardRuntime", () => ({
  startDashboardRuntime: mocks.startRuntime,
  stopDashboardRuntime: vi.fn(),
  clearDashboardRuntimeState: vi.fn(),
}));

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
