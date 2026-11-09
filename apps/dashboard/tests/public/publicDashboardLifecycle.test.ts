import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import PublicDashboardHarness from "../components/PublicDashboardHarness.svelte";
import PublicDashboardRouteHarness from "../components/PublicDashboardRouteHarness.svelte";
import { createRemoteAccessFixture } from "./fixtures/remoteConnections";

let fixture: ReturnType<typeof createRemoteAccessFixture>;
vi.mock("../../src/modes/public/remote-access", async () => ({
  ...(await import("../../src/modes/public/remote-access/remoteAccess")),
  listPublicDevices: (signal: AbortSignal) => fixture.remote.listPublicDevices(signal),
  createRemoteConnection: (handle: string, onSignInRequired: () => void) =>
    fixture.remote.createRemoteConnection(handle, onSignInRequired),
}));

beforeEach(() => {
  fixture = createRemoteAccessFixture();
  window.history.replaceState({}, "", "/office-mac");
});

test("retries a failed device list request", async () => {
  vi.mocked(fixture.remote.listPublicDevices).mockRejectedValueOnce(new Error("503"));
  render(PublicDashboardHarness, { props: { handle: "" } });
  await fireEvent.click(await screen.findByRole("button", { name: "Try again" }));
  await userEvent.click(await screen.findByRole("button", { name: "Sidebar device" }));
  const devices = await screen.findAllByRole("menuitemradio", { hidden: true });
  expect(devices.map((device) => device.textContent)).toEqual([
    expect.stringContaining("Office Mac"),
    expect.stringContaining("Travel Laptop"),
  ]);
});

test("cross-device navigation preserves the AppShell and both device connections", async () => {
  const view = render(PublicDashboardHarness, { props: { handle: "office-mac" } });
  await screen.findByText("Dashboard content");
  await waitFor(() => expect(fixture.streams.size).toBe(2));
  const shell = document.querySelector('[data-slot="sidebar-wrapper"]');
  expect(shell).not.toBeNull();
  const firstSignals = [...fixture.signals.values()].flat();
  await view.rerender({ handle: "travel-laptop" });
  await screen.findByText("Dashboard content");
  expect(document.querySelector('[data-slot="sidebar-wrapper"]')).toBe(shell);
  expect(firstSignals.every((signal) => !signal.aborted)).toBe(true);
  expect(fixture.connect).toHaveBeenCalledTimes(2);
  await view.rerender({ handle: "" });
  await screen.findByRole("button", { name: "Sidebar device" });
  expect(document.querySelector('[data-slot="sidebar-wrapper"]')).toBe(shell);
  expect(firstSignals.every((signal) => !signal.aborted)).toBe(true);
});

test("navigation within the same device preserves mounted page state", async () => {
  const view = render(PublicDashboardRouteHarness, { props: { data: { handle: "office-mac" } } });
  const draft = await screen.findByRole("textbox", { name: "Draft" });
  await fireEvent.input(draft, { target: { value: "Unsent message" } });
  await view.rerender({ data: { handle: "office-mac" } });
  expect(screen.getByRole("textbox", { name: "Draft" })).toBe(draft);
  expect(draft).toHaveValue("Unsent message");
  expect(fixture.connect).toHaveBeenCalledTimes(2);
});

test("a direct offline target can be retried while another device connects in the background", async () => {
  fixture.connect.mockRejectedValueOnce(new Error("handshake failed"));
  render(PublicDashboardHarness, { props: { handle: "office-mac" } });
  const retry = await screen.findByRole("button", { name: "Try again" });
  await waitFor(() => expect(fixture.streams.has("travel-laptop")).toBe(true));
  expect(screen.queryByText("Dashboard content")).not.toBeInTheDocument();
  await fireEvent.click(retry);
  await screen.findByText("Dashboard content");
  expect(fixture.signals.get("travel-laptop")).toHaveLength(1);
});

test("unmount cancels pending handshakes and ignores late results", async () => {
  let finish!: () => void;
  fixture.connect.mockImplementationOnce(
    (handle) =>
      new Promise((resolve) => {
        finish = () => resolve(fixture.target(handle));
      }),
  );
  const view = render(PublicDashboardHarness, { props: { handle: "office-mac" } });
  await waitFor(() => expect(fixture.connect).toHaveBeenCalledTimes(2));
  view.unmount();
  finish();
  await Promise.resolve();
  expect([...fixture.signals.values()].flat().every((signal) => signal.aborted)).toBe(true);
  expect(fixture.streams.has("office-mac")).toBe(false);
});

test("focus refresh discovers new devices without reconnecting existing ones", async () => {
  render(PublicDashboardHarness, { props: { handle: "office-mac" } });
  await screen.findByText("Dashboard content");
  fixture.listed.push({ handle: "home-server", name: "Home server" });
  await fireEvent.focus(window);
  await waitFor(() => expect(fixture.streams.has("home-server")).toBe(true));
  expect(fixture.signals.get("office-mac")).toHaveLength(1);
  expect(fixture.signals.get("travel-laptop")).toHaveLength(1);
});

test("central login expiry offers sign-in and prevents UI or focus retries", async () => {
  render(PublicDashboardHarness, { props: { handle: "office-mac" } });
  await screen.findByText("Dashboard content");
  fixture.authorizationFailures.get("travel-laptop")!();
  const signIn = await screen.findByRole("link", { name: "Sign in" });
  expect(signIn.getAttribute("href")).toContain("https://pontia.dev/login?");
  expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
  await fireEvent.focus(window);
  expect(fixture.connect).toHaveBeenCalledTimes(2);
  expect([...fixture.signals.values()].flat().every((signal) => signal.aborted)).toBe(true);
});
