import { createContext } from "svelte";
import type { DeviceStatusView } from "../components/devices/DeviceStatus.svelte";
import type { DeviceRuntime } from "./runtime";

export interface HostDevice {
  id: string;
  handle: string;
  name: string;
  state: "online" | "offline" | "connecting" | "sign_in_required";
  runtime: DeviceRuntime | null;
}

export interface DashboardHost {
  readonly mode: "local" | "public";
  readonly devices: HostDevice[];
  readonly activeDevice: HostDevice | null;
  readonly previousDevice: HostDevice | null;
  readonly canSwitchDevice: boolean;
  readonly routeScope: string | undefined;
  readonly statusView: DeviceStatusView | null;
  readonly signInUrl: string | undefined;
  readonly loadError: string | null;
  readonly onboarding: boolean;
  readonly loadingDevices: boolean;
  readonly connecting: boolean;
  readonly checkingDevices: boolean;
  selectDevice(handle: string, options?: { keepMobileTab?: boolean }): void;
  retryActiveOrRefresh(): void;
  retryDevice(handle: string): void;
  checkDevices(): Promise<void>;
  openNewChat(): void;
  openSettings(): void;
}

export const [useDashboardHost, provideDashboardHost] = createContext<DashboardHost>();
