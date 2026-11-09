import { render, screen } from "@testing-library/svelte";
import { expect, test, vi } from "vitest";
import SettingsCommonPage from "../../src/modes/public/SettingsCommonPage.svelte";

const remote = vi.hoisted(() => ({
  getCloudProfile: vi.fn(),
  cloudAccountUrl: () => "https://pontia.dev/settings/account",
}));
vi.mock("../../src/modes/public/remoteAccessLoader.svelte", () => ({
  loadRemoteAccess: async () => remote,
}));

test("shows the signed-in Cloud user and links to the Cloud account", async () => {
  remote.getCloudProfile.mockResolvedValue({ displayName: "Ada", avatarUrl: null });
  render(SettingsCommonPage);
  expect(await screen.findByText("Ada")).toBeInTheDocument();
  expect(screen.getByRole("link", { name: /Manage account/ })).toHaveAttribute(
    "href",
    "https://pontia.dev/settings/account",
  );
});

test("reports a profile that cannot be loaded", async () => {
  remote.getCloudProfile.mockRejectedValue(new Error("offline"));
  render(SettingsCommonPage);
  expect(await screen.findByText("Could not load your Pontia Cloud account.")).toBeInTheDocument();
});
