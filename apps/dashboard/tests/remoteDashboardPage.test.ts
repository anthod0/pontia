import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, test, vi } from "vitest";
import DevicesPage from "../src/components/devices/DevicesPage.svelte";
import RemoteDashboardPage from "../src/components/devices/RemoteDashboardPage.svelte";
import { match as matchHandle } from "../src/params/handle";
import { initialRemoteDashboardState, isValidDeviceHandle } from "../src/lib/remoteDashboard";

const handle = "office-mac";

beforeEach(() => {
  window.history.pushState({}, "", `/${handle}`);
  localStorage.clear();
  vi.clearAllMocks();
});

describe("device handle", () => {
  test("accepts 4-48 character handles that start with a lowercase letter", () => {
    expect(isValidDeviceHandle(handle)).toBe(true);
    expect(isValidDeviceHandle("a_b-")).toBe(true);
    expect(isValidDeviceHandle("a".repeat(48))).toBe(true);
    expect(isValidDeviceHandle("abc")).toBe(false);
    expect(isValidDeviceHandle("a".repeat(49))).toBe(false);
    expect(isValidDeviceHandle("Office-Mac")).toBe(false);
    expect(isValidDeviceHandle("2office-mac")).toBe(false);
    expect(isValidDeviceHandle("-office-mac")).toBe(false);
    expect(isValidDeviceHandle("office.mac")).toBe(false);
    expect(isValidDeviceHandle("workflows")).toBe(false);
  });

  test("maps valid targets to connecting and invalid targets to invalid", () => {
    expect(initialRemoteDashboardState(handle)).toBe("connecting");
    expect(initialRemoteDashboardState("2not_a_handle")).toBe("invalid");
  });

  test("only enables the optional handle route in public builds", () => {
    expect(matchHandle(handle)).toBe(import.meta.env.VITE_DASHBOARD_MODE === "public");
    expect(matchHandle("workflows")).toBe(false);
  });
});

test("renders the public device list empty state", () => {
  render(DevicesPage);

  expect(screen.getByRole("heading", { name: "Devices" })).toBeInTheDocument();
  expect(screen.getByText("No devices yet")).toBeInTheDocument();
  expect(screen.queryByLabelText(/bearer token/i)).not.toBeInTheDocument();
});

test("renders valid devices as handle-scoped dashboard links", () => {
  render(DevicesPage, {
    props: {
      devices: [
        { handle, name: "Office Mac" },
        { handle: "2invalid_handle", name: "Invalid" },
      ],
      openAction: (deviceHandle: string) =>
        `https://pontia.dev/api/dashboard/devices/${deviceHandle}/bootstrap`,
    },
  });

  expect(screen.getByRole("button", { name: "Open Dashboard" }).closest("form")).toHaveAttribute(
    "action",
    `https://pontia.dev/api/dashboard/devices/${handle}/bootstrap`,
  );
  expect(screen.getByRole("button", { name: "Open Dashboard" }).closest("form")).toHaveAttribute(
    "method",
    "POST",
  );
  expect(screen.getByText("Office Mac")).toBeInTheDocument();
  expect(screen.queryByText("Invalid")).not.toBeInTheDocument();
  expect(screen.queryByText("Online")).not.toBeInTheDocument();
  expect(screen.queryByText("Offline")).not.toBeInTheDocument();
});

test.each([
  ["connecting", "Connecting to device"],
  ["authorization-required", "Authorization required"],
  ["unavailable", "Device unavailable"],
] as const)("renders the %s state", (state, heading) => {
  render(RemoteDashboardPage, { props: { handle, state } });
  expect(screen.getByRole("heading", { name: heading })).toBeInTheDocument();
});

test("enters the existing dashboard shell when the device is available", () => {
  render(RemoteDashboardPage, { props: { handle, state: "available" } });

  expect(screen.getByText("Pontia")).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Connecting to device" })).not.toBeInTheDocument();
});

test("invalid handles override any requested connection state", () => {
  render(RemoteDashboardPage, {
    props: { handle: "2not_a_handle", state: "available" },
  });

  expect(screen.getByRole("heading", { name: "Invalid device" })).toBeInTheDocument();
  expect(screen.queryByText("Pontia")).not.toBeInTheDocument();
});

test("exposes retry and reauthorization actions without inventing credential handling", async () => {
  const onRetry = vi.fn();
  const unavailable = render(RemoteDashboardPage, {
    props: { handle, state: "unavailable", onRetry },
  });
  await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  expect(onRetry).toHaveBeenCalledOnce();
  unavailable.unmount();

  render(RemoteDashboardPage, {
    props: {
      handle,
      state: "authorization-required",
      reauthorizationUrl: `https://pontia.dev/api/dashboard/devices/${handle}/bootstrap`,
    },
  });
  const form = screen.getByRole("button", { name: "Authorize again" }).closest("form");
  expect(form).toHaveAttribute(
    "action",
    `https://pontia.dev/api/dashboard/devices/${handle}/bootstrap`,
  );
  expect(form).toHaveAttribute("method", "POST");
});

test("does not read or persist a local token on a public page", () => {
  localStorage.setItem("pontia.externalApiToken", "local-mode-value");
  const getItem = vi.spyOn(Storage.prototype, "getItem");
  const setItem = vi.spyOn(Storage.prototype, "setItem");
  const fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);

  render(RemoteDashboardPage, { props: { handle, state: "connecting" } });

  expect(screen.queryByLabelText(/bearer token/i)).not.toBeInTheDocument();
  expect(screen.queryByText(/external api token/i)).not.toBeInTheDocument();
  expect(getItem).not.toHaveBeenCalled();
  expect(setItem).not.toHaveBeenCalled();
  expect(fetchMock).not.toHaveBeenCalled();
});
