import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import PublicSettingsCommonHarness from "../components/PublicSettingsCommonHarness.svelte";
import type { ConnectedDevice } from "../../src/modes/public/deviceConnections";
import type { DeviceList } from "../../src/modes/public/deviceListContext";

const remote = vi.hoisted(() => ({
  getCloudProfile: vi.fn(),
  cloudAccountUrl: () => "https://pontia.dev/settings/account",
  cloudDevicesUrl: () => "https://pontia.dev/settings/devices",
}));
vi.mock("../../src/modes/public/remoteAccessLoader.svelte", () => ({
  loadRemoteAccess: async () => remote,
}));

function deviceList(devices: ConnectedDevice[] = []): DeviceList {
  return {
    devices: () => devices,
    retry: vi.fn(),
  };
}

test("shows the signed-in Cloud user and links to the Cloud account", async () => {
  remote.getCloudProfile.mockResolvedValue({ displayName: "Ada", avatarUrl: null });
  render(PublicSettingsCommonHarness, { list: deviceList() });
  expect(await screen.findByText("Ada")).toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Account" })).toHaveTextContent("Ada");
  expect(screen.getByRole("region", { name: "Devices" })).toBeInTheDocument();
  expect(screen.getByRole("link", { name: /Manage account/ })).toHaveAttribute(
    "href",
    "https://pontia.dev/settings/account",
  );
});

test("reports a profile that cannot be loaded", async () => {
  remote.getCloudProfile.mockRejectedValue(new Error("offline"));
  render(PublicSettingsCommonHarness, { list: deviceList() });
  expect(await screen.findByText("Could not load your Pontia Cloud account.")).toBeInTheDocument();
});

test("lists devices with their state and links to Cloud device management", async () => {
  remote.getCloudProfile.mockResolvedValue({ displayName: "Ada", avatarUrl: null });
  const list = deviceList([
    { handle: "office-mac", name: "Office Mac", state: "online", runtime: null },
    { handle: "travel-laptop", name: "Travel Laptop", state: "offline", runtime: null },
  ]);
  render(PublicSettingsCommonHarness, { list });

  expect(await screen.findByRole("link", { name: /Manage devices/ })).toHaveAttribute(
    "href",
    "https://pontia.dev/settings/devices",
  );
  const [office, travel] = within(screen.getByRole("list", { name: "Devices" })).getAllByRole(
    "listitem",
  );
  expect(office).toHaveTextContent("Office Mac");
  expect(office).toHaveTextContent("Online");
  expect(within(office).queryByRole("button")).not.toBeInTheDocument();
  expect(travel).toHaveTextContent("Offline");

  await userEvent.click(within(travel).getByRole("button", { name: "Retry Travel Laptop" }));
  expect(list.retry).toHaveBeenCalledWith("travel-laptop");
});

test("shows an empty device list", async () => {
  remote.getCloudProfile.mockResolvedValue({ displayName: "Ada", avatarUrl: null });
  render(PublicSettingsCommonHarness, { list: deviceList() });
  expect(await screen.findByText("No devices registered.")).toBeInTheDocument();
});
